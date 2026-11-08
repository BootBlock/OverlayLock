//! `cargo xtask dist`: builds the release and lays it out in one of two ways.
//!
//! - With no `--zip`, it installs this machine's target, or each target `--target` names, into
//!   `!Distribution\` at the repository root: the executable named for its target, such as
//!   `overlay-lock_x64.exe`, with `LICENSE` and `README.md`, ready to run from there, for example
//!   from a Stream Deck button. It replaces only those files, so the folder and each path stay put
//!   between builds, and it removes the `overlay-lock.exe` that earlier versions installed.
//! - With `--zip <folder>`, it writes what a GitHub release publishes into that folder: one zip per
//!   target and `SHA256SUMS.txt`. Each zip holds `LICENSE`, `README.md` and the executable named
//!   for its target, as installed. CI and the release workflow run this form.
//!
//! Every executable carries its target in its name, so the builds for each processor can share a
//! folder.
//!
//! A target is named by its short name (`x64`, `x86` or `arm64`), which the zip names use, or by
//! its Rust target triple.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};

/// The folder, under the repository root, that receives the installed build.
const INSTALL_DIR: &str = "!Distribution";

// `Target`, `TARGETS` and `exe_name`, shared with the build script's version resource.
include!("../../release_targets.rs");

/// The target of the machine this task runs on, which the installed build uses by default.
const HOST_TARGET: Target = TARGETS[if cfg!(target_arch = "aarch64") {
    2
} else if cfg!(target_arch = "x86") {
    1
} else {
    0
}];

/// The file name of the executable Cargo builds, and of the copy earlier versions of this task
/// installed into `!Distribution\`.
const EXE: &str = "overlay-lock.exe";

/// The files shipped beside the executable, relative to the repository root.
const EXTRA_FILES: [&str; 2] = ["LICENSE", "README.md"];

const USAGE: &str = "Usage: cargo xtask dist [--target <target>] [--zip <folder> [--target <target>]...]\n\n\
  (no --zip)         build and install overlay-lock_<target>.exe, LICENSE and README.md into\n\
                     !Distribution\\, ready to run from there\n\
  --target <target>  the target to build: x64, x86 or arm64, or its target triple; repeat it for\n\
                     more (default: this machine's, or all three with --zip)\n\
  --zip <folder>     write one release zip per target and SHA256SUMS.txt into <folder>, relative\n\
                     to the repository root\n\n\
Each target is built with `cargo build --release --locked`.";

/// Runs the `dist` command with the arguments that follow it.
pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut targets: Vec<Target> = Vec::new();
    let mut zip_dir: Option<PathBuf> = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let target = parse_target(&args.next().ok_or("--target needs a target")?)?;
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            "--zip" => zip_dir = Some(args.next().ok_or("--zip needs a folder")?.into()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument '{other}'\n\n{USAGE}")),
        }
    }

    let root = repository_root()?;
    if let Some(folder) = zip_dir {
        if targets.is_empty() {
            targets = TARGETS.to_vec();
        }
        return write_zips(&root, &root.join(folder), &targets);
    }
    if targets.is_empty() {
        targets.push(HOST_TARGET);
    }
    let mut builds = Vec::with_capacity(targets.len());
    for target in targets {
        builds.push((target, build(&root, target)?));
    }
    install(&root, &builds)
}

/// Finds the release target a `--target` value names, by its short name or its triple, ignoring
/// case.
fn parse_target(value: &str) -> Result<Target, String> {
    TARGETS
        .into_iter()
        .find(|target| {
            target.name.eq_ignore_ascii_case(value) || target.triple.eq_ignore_ascii_case(value)
        })
        .ok_or_else(|| {
            let names: Vec<&str> = TARGETS.iter().map(|target| target.name).collect();
            format!(
                "unknown target '{value}': use {} or its target triple",
                names.join(", ")
            )
        })
}

/// Copies each built executable, under its target's name, and the extra files into
/// `!Distribution\`, replacing only those files, then removes the `overlay-lock.exe` that earlier
/// versions of this task installed there.
///
/// Each file is written beside its destination first and then renamed over it, so a failed copy
/// never leaves a half-written executable. A running executable cannot be replaced: end the lock
/// and run the task again.
fn install(root: &Path, builds: &[(Target, PathBuf)]) -> Result<(), String> {
    let folder = root.join(INSTALL_DIR);
    fs::create_dir_all(&folder)
        .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;
    let executables = builds
        .iter()
        .map(|(target, exe)| (exe.clone(), exe_name(*target), true));
    let extras = EXTRA_FILES
        .iter()
        .map(|name| (root.join(name), (*name).to_string(), false));
    for (source, name, is_exe) in executables.chain(extras) {
        let destination = folder.join(&name);
        let staged = folder.join(format!("{name}.new"));
        fs::copy(&source, &staged)
            .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
        if let Err(error) = fs::rename(&staged, &destination) {
            let _ = fs::remove_file(&staged);
            let hint = if is_exe {
                " (is OverlayLock running from there? End the lock and run this again)"
            } else {
                ""
            };
            return Err(format!(
                "cannot replace {}: {error}{hint}",
                destination.display()
            ));
        }
        println!("{}", destination.display());
    }
    remove_superseded(&folder)
}

/// Removes the `overlay-lock.exe` that earlier versions of this task installed, so a shortcut or a
/// Stream Deck button still pointing at it fails instead of starting an old build.
fn remove_superseded(folder: &Path) -> Result<(), String> {
    let superseded = folder.join(EXE);
    match fs::remove_file(&superseded) {
        Ok(()) => {
            println!(
                "removed {}: point shortcuts at the overlay-lock_<target>.exe beside it",
                superseded.display()
            );
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot remove {}: {error} (is OverlayLock running from there? End the lock and run \
             this again)",
            superseded.display()
        )),
    }
}

/// Builds each target and writes its release zip and `SHA256SUMS.txt` into `folder`. Other files
/// in `folder` are left alone.
fn write_zips(root: &Path, folder: &Path, targets: &[Target]) -> Result<(), String> {
    let version = package_version(root)?;
    let mut executables = Vec::with_capacity(targets.len());
    for target in targets {
        executables.push(build(root, *target)?);
    }
    fs::create_dir_all(folder)
        .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;
    let mut sums = String::new();
    for (target, exe) in targets.iter().zip(&executables) {
        let name = zip_name(&version, *target);
        let zip = folder.join(&name);
        if zip.exists() {
            fs::remove_file(&zip)
                .map_err(|error| format!("cannot replace {}: {error}", zip.display()))?;
        }
        package(root, exe, &exe_name(*target), &zip)?;
        let bytes =
            fs::read(&zip).map_err(|error| format!("cannot read {}: {error}", zip.display()))?;
        sums.push_str(&sum_line(&sha256(&bytes)?, &name));
        println!("{}", zip.display());
    }
    let sums_path = folder.join("SHA256SUMS.txt");
    fs::write(&sums_path, sums)
        .map_err(|error| format!("cannot write {}: {error}", sums_path.display()))?;
    println!("{}", sums_path.display());
    Ok(())
}

/// Gets the repository root: the parent of the `xtask` package.
fn repository_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot find the repository root".to_string())
}

/// Gets the version of the `overlay-lock` package as Cargo resolves it, through `cargo pkgid`.
fn package_version(root: &Path) -> Result<String, String> {
    let output = Command::new(cargo())
        .current_dir(root)
        .args(["pkgid", "--package", "overlay-lock"])
        .output()
        .map_err(|error| format!("cannot start cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo pkgid failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let pkgid = String::from_utf8_lossy(&output.stdout);
    version_from_pkgid(pkgid.trim())
        .map(ToString::to_string)
        .ok_or_else(|| {
            format!(
                "cannot read a version from the package ID '{}'",
                pkgid.trim()
            )
        })
}

/// Gets the version from a Cargo package ID specification: the part after `#`, without a leading
/// `name@` (Cargo leaves the name out when it matches the last path segment). Returns `None` if
/// there is no `#` or nothing after it.
fn version_from_pkgid(pkgid: &str) -> Option<&str> {
    let (_, fragment) = pkgid.rsplit_once('#')?;
    let version = fragment
        .rsplit_once('@')
        .map_or(fragment, |(_, version)| version);
    (!version.is_empty()).then_some(version)
}

/// Gets the Cargo that runs this task, or `cargo` from `PATH` outside `cargo run`.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// Gets the zip file name for a version and a target, such as `overlay-lock-0.2.0-x64.zip`.
fn zip_name(version: &str, target: Target) -> String {
    format!("overlay-lock-{version}-{}.zip", target.name)
}

/// Builds the release executable for `target` and returns its path, as Cargo reports it, so a
/// `CARGO_TARGET_DIR` or `build.target-dir` setting is followed.
fn build(root: &Path, target: Target) -> Result<PathBuf, String> {
    let output = Command::new(cargo())
        .current_dir(root)
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "overlay-lock",
            "--target",
            target.triple,
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cannot start cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo build for {} failed ({})",
            target.triple, output.status
        ));
    }
    let messages = String::from_utf8_lossy(&output.stdout);
    let exe = messages
        .lines()
        .filter(|line| line.contains(r#""reason":"compiler-artifact""#))
        .filter_map(|line| json_string_field(line, "executable"))
        .find(|path| Path::new(path).file_name() == Some(EXE.as_ref()))
        .ok_or_else(|| format!("cargo build for {} reported no {EXE}", target.triple))?;
    Ok(PathBuf::from(exe))
}

/// Gets the string value of the first `"key":"..."` member in one line of Cargo's JSON messages,
/// with its JSON escapes decoded, or `None` if the line has no such string member.
fn json_string_field(line: &str, key: &str) -> Option<String> {
    let start = line.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let mut value = String::new();
    let mut chars = line[start..].chars();
    loop {
        match chars.next()? {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                'b' => value.push('\u{8}'),
                'f' => value.push('\u{c}'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let unit = u16::from_str_radix(&hex, 16).ok()?;
                    value.push(char::from_u32(u32::from(unit))?);
                }
                _ => return None,
            },
            other => value.push(other),
        }
    }
}

/// Writes the zip: the executable as `exe_name`, then the extra files from the repository root.
///
/// It uses the `tar.exe` that ships in System32 (Windows 10 1803 and later), which writes a zip
/// when the archive name ends in `.zip`. The one on `PATH` may be GNU tar, which cannot. That
/// `tar.exe` cannot rename an entry (no `-s`), so the executable is copied under its zip name into
/// a staging folder beside the zip first, and the folder is removed afterwards.
fn package(root: &Path, exe: &Path, exe_name: &str, zip: &Path) -> Result<(), String> {
    if !exe.is_file() {
        return Err(format!("{} is missing", exe.display()));
    }
    let mut stage = zip.as_os_str().to_owned();
    stage.push(".stage");
    let stage = PathBuf::from(stage);
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)
        .map_err(|error| format!("cannot create {}: {error}", stage.display()))?;
    let result = fs::copy(exe, stage.join(exe_name))
        .map_err(|error| format!("cannot copy {}: {error}", exe.display()))
        .and_then(|_| tar_zip(root, &stage, exe_name, zip));
    let _ = fs::remove_dir_all(&stage);
    result
}

/// Runs the System32 `tar.exe` to write `zip` from `exe_name` in `stage` and the extra files.
fn tar_zip(root: &Path, stage: &Path, exe_name: &str, zip: &Path) -> Result<(), String> {
    let system_root = std::env::var_os("SystemRoot").ok_or("SystemRoot is not set")?;
    let tar = Path::new(&system_root).join("System32").join("tar.exe");
    let status = Command::new(&tar)
        .arg("-a")
        .arg("-c")
        .arg("-f")
        .arg(zip)
        .arg("-C")
        .arg(stage)
        .arg(exe_name)
        .arg("-C")
        .arg(root)
        .args(EXTRA_FILES)
        .status()
        .map_err(|error| format!("cannot start {}: {error}", tar.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} failed for {} ({status})",
            tar.display(),
            zip.display()
        ))
    }
}

/// Computes the SHA-256 digest of `bytes` with the Windows CNG provider.
fn sha256(bytes: &[u8]) -> Result<[u8; 32], String> {
    let mut digest = [0u8; 32];
    // SAFETY: the pseudo-handle needs no opening (Windows 10 and later); the input and output
    // slices are valid for the call, and the output is the SHA-256 digest size.
    let status = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, bytes, &mut digest) };
    status
        .ok()
        .map_err(|error| format!("SHA-256 failed: {error}"))?;
    Ok(digest)
}

/// Formats one `SHA256SUMS.txt` line, as `sha256sum` writes it: lower-case hex, two spaces, the
/// file name, a line feed.
fn sum_line(digest: &[u8; 32], name: &str) -> String {
    let mut line = String::with_capacity(digest.len() * 2 + name.len() + 3);
    for byte in digest {
        let _ = write!(line, "{byte:02x}");
    }
    line.push_str("  ");
    line.push_str(name);
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::{
        HOST_TARGET, INSTALL_DIR, TARGETS, exe_name, install, json_string_field, package,
        parse_target, sha256, sum_line, version_from_pkgid, zip_name,
    };
    use std::fs;

    /// The two forms `cargo pkgid` printed for this workspace (Cargo 1.98).
    #[test]
    fn version_from_pkgid_reads_both_cargo_forms() {
        assert_eq!(
            version_from_pkgid("path+file:///P:/Source/Rust/OverlayLock#overlay-lock@0.1.0"),
            Some("0.1.0")
        );
        assert_eq!(
            version_from_pkgid("path+file:///P:/Source/Rust/OverlayLock/xtask#0.1.0"),
            Some("0.1.0")
        );
        assert_eq!(
            version_from_pkgid(
                "registry+https://github.com/rust-lang/crates.io-index#windows@0.62.2"
            ),
            Some("0.62.2")
        );
    }

    #[test]
    fn version_from_pkgid_is_none_without_a_fragment() {
        assert_eq!(
            version_from_pkgid("path+file:///P:/Source/Rust/OverlayLock"),
            None
        );
        assert_eq!(version_from_pkgid("path+file:///x#"), None);
        assert_eq!(version_from_pkgid("path+file:///x#name@"), None);
    }

    /// Runs the real `cargo pkgid`, so a change in Cargo's output format fails here first. The
    /// root manifest must hold the version it gives, as the `[package]` table's version line.
    #[test]
    fn package_version_is_the_root_manifest_version() {
        let root = super::repository_root().expect("the root is found");
        let version = super::package_version(&root).expect("cargo pkgid gives a version");
        let manifest = include_str!("../../Cargo.toml");
        let package = manifest
            .split_once("[package]")
            .and_then(|(_, rest)| rest.split_once("\n[").map(|(table, _)| table))
            .expect("the root manifest has a [package] table");
        assert!(
            package.contains(&format!("\nversion = \"{version}\"")),
            "{version} is not the [package] version"
        );
    }

    /// A line in the form `cargo build --message-format=json` printed for this workspace.
    #[test]
    fn json_string_field_decodes_a_windows_path() {
        let line = r#"{"reason":"compiler-artifact","package_id":"path+file:///P:/x#overlay-lock@0.1.0","filenames":["P:\\x\\target\\release\\overlay-lock.exe"],"executable":"P:\\x\\target\\release\\overlay-lock.exe","fresh":true}"#;
        assert_eq!(
            json_string_field(line, "executable").as_deref(),
            Some(r"P:\x\target\release\overlay-lock.exe")
        );
    }

    #[test]
    fn json_string_field_decodes_escapes_and_rejects_bad_input() {
        assert_eq!(
            json_string_field(r#"{"k":"a\"b\/c\u0041"}"#, "k").as_deref(),
            Some("a\"b/cA")
        );
        assert_eq!(
            json_string_field(r#"{"executable":null}"#, "executable"),
            None
        );
        assert_eq!(json_string_field(r#"{"k":"no end"#, "k"), None);
        assert_eq!(json_string_field(r#"{"k":"bad \q"}"#, "k"), None);
    }

    /// Installs each executable under its target's name and replaces the extra files, keeps
    /// anything else in the folder, removes the superseded `overlay-lock.exe`, and leaves no staged
    /// `.new` file behind.
    #[test]
    fn install_names_each_executable_for_its_target() {
        let root =
            std::env::temp_dir().join(format!("overlay-lock-dist-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = root.join(INSTALL_DIR);
        fs::create_dir_all(&folder).expect("the test folder is created");
        fs::write(root.join("LICENSE"), "new licence").expect("written");
        fs::write(root.join("README.md"), "new readme").expect("written");
        let x64 = root.join("x64.exe");
        let arm64 = root.join("arm64.exe");
        fs::write(&x64, "new x64").expect("written");
        fs::write(&arm64, "new arm64").expect("written");
        fs::write(folder.join("overlay-lock.exe"), "old exe").expect("written");
        fs::write(folder.join("overlay-lock_x64.exe"), "old x64").expect("written");
        fs::write(folder.join("LICENSE"), "old licence").expect("written");
        fs::write(folder.join("notes.txt"), "mine").expect("written");

        install(&root, &[(TARGETS[0], x64), (TARGETS[2], arm64)]).expect("the install succeeds");

        let read = |name: &str| fs::read_to_string(folder.join(name)).expect("readable");
        assert_eq!(read("overlay-lock_x64.exe"), "new x64");
        assert_eq!(read("overlay-lock_arm64.exe"), "new arm64");
        assert_eq!(read("LICENSE"), "new licence");
        assert_eq!(read("README.md"), "new readme");
        assert_eq!(read("notes.txt"), "mine");
        let mut names: Vec<String> = fs::read_dir(&folder)
            .expect("listable")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "LICENSE",
                "README.md",
                "notes.txt",
                "overlay-lock_arm64.exe",
                "overlay-lock_x64.exe"
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The release ships one zip per target, named by the target's short name.
    #[test]
    fn zip_names_use_the_short_target_names() {
        let names: Vec<String> = TARGETS
            .into_iter()
            .map(|target| zip_name("0.2.0", target))
            .collect();
        assert_eq!(
            names,
            [
                "overlay-lock-0.2.0-x64.zip",
                "overlay-lock-0.2.0-x86.zip",
                "overlay-lock-0.2.0-arm64.zip"
            ]
        );
    }

    /// Each zip's executable is named for its target, so the three builds can share a folder.
    #[test]
    fn exe_names_carry_the_target() {
        let names: Vec<String> = TARGETS.into_iter().map(exe_name).collect();
        assert_eq!(
            names,
            [
                "overlay-lock_x64.exe",
                "overlay-lock_x86.exe",
                "overlay-lock_arm64.exe"
            ]
        );
    }

    /// Runs the real System32 `tar.exe`: the zip holds the executable under its new name and the
    /// extra files, and the staging folder is gone afterwards.
    #[test]
    fn package_renames_the_executable_and_cleans_up() {
        let dir =
            std::env::temp_dir().join(format!("overlay-lock-zip-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = dir.join("root");
        let out = dir.join("out");
        fs::create_dir_all(&root).expect("the root is created");
        fs::create_dir_all(&out).expect("the output folder is created");
        fs::write(root.join("LICENSE"), "licence").expect("written");
        fs::write(root.join("README.md"), "readme").expect("written");
        let exe = dir.join("overlay-lock.exe");
        fs::write(&exe, "exe").expect("written");
        let zip = out.join("test.zip");

        package(&root, &exe, "overlay-lock_x64.exe", &zip).expect("the zip is written");

        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set");
        let listing = std::process::Command::new(
            std::path::Path::new(&system_root)
                .join("System32")
                .join("tar.exe"),
        )
        .arg("-t")
        .arg("-f")
        .arg(&zip)
        .output()
        .expect("tar lists the zip");
        let mut entries: Vec<String> = String::from_utf8_lossy(&listing.stdout)
            .lines()
            .map(ToString::to_string)
            .collect();
        entries.sort();
        assert_eq!(entries, ["LICENSE", "README.md", "overlay-lock_x64.exe"]);
        let renamed = std::process::Command::new(
            std::path::Path::new(&system_root)
                .join("System32")
                .join("tar.exe"),
        )
        .args(["-x", "-O", "-f"])
        .arg(&zip)
        .arg("overlay-lock_x64.exe")
        .output()
        .expect("tar extracts the executable");
        assert_eq!(
            renamed.stdout, b"exe",
            "the renamed entry is the built executable"
        );
        let left: Vec<_> = fs::read_dir(&out)
            .expect("listable")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(left, ["test.zip"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_target_takes_a_short_name_or_a_triple() {
        for target in TARGETS {
            assert_eq!(parse_target(target.name), Ok(target));
            assert_eq!(parse_target(target.triple), Ok(target));
            assert_eq!(parse_target(&target.name.to_uppercase()), Ok(target));
        }
        let error = parse_target("x86_64-pc-windows-gnu").expect_err("not a release target");
        assert!(error.contains("x64, x86, arm64"), "{error}");
    }

    /// The installed build defaults to the processor this task was built for.
    #[test]
    fn host_target_matches_this_processor() {
        let arch = HOST_TARGET
            .triple
            .split('-')
            .next()
            .expect("a triple has an architecture");
        let expected = if cfg!(target_arch = "x86") {
            "i686"
        } else {
            std::env::consts::ARCH
        };
        assert_eq!(arch, expected);
    }

    /// Every release target is built with the static C runtime, so no zip needs the Visual C++
    /// runtime installed.
    #[test]
    fn every_target_links_the_c_runtime_statically() {
        let config = include_str!("../../.cargo/config.toml").replace("\r\n", "\n");
        for target in TARGETS {
            let table = format!(
                "[target.{}]\nrustflags = [\"-C\", \"target-feature=+crt-static\"]",
                target.triple
            );
            assert!(
                config.contains(&table),
                "{} has no static C runtime",
                target.triple
            );
        }
    }

    /// FIPS 180-2, appendix B.1: SHA-256 of "abc".
    #[test]
    fn sha256_matches_the_standard_vector() {
        let line = sum_line(&sha256(b"abc").expect("CNG hashes"), "abc.zip");
        assert_eq!(
            line,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  abc.zip\n"
        );
    }
}
