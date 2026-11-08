//! `cargo xtask dist`: builds the release for each target and writes the files a GitHub release
//! publishes into `!Distribution/` at the repository root: one zip per target, holding
//! `overlay-lock.exe`, `LICENSE` and `README.md`, and `SHA256SUMS.txt`. The release workflow runs
//! the same command, so a local build and a published release hold the same files.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};

/// The folder, under the repository root, that receives the release files.
const OUTPUT_DIR: &str = "!Distribution";

/// The targets a release ships, in the order they are built.
const TARGETS: [&str; 2] = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"];

/// The files each zip holds besides the executable, relative to the repository root.
const EXTRA_FILES: [&str; 2] = ["LICENSE", "README.md"];

const USAGE: &str = "Usage: cargo xtask dist [--target <triple>]...\n\n\
  --target <triple>  build only this target; repeat for more (default: x86_64-pc-windows-msvc\n\
                     and aarch64-pc-windows-msvc)\n\n\
Builds each target with `cargo build --release --locked`, then replaces !Distribution\\ with one\n\
zip per target and SHA256SUMS.txt.";

/// Runs the `dist` command with the arguments that follow it.
pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut targets: Vec<String> = Vec::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let target = args.next().ok_or("--target needs a target triple")?;
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument '{other}'\n\n{USAGE}")),
        }
    }
    if targets.is_empty() {
        targets = TARGETS.iter().map(ToString::to_string).collect();
    }

    let root = repository_root()?;
    let version = package_version(&root)?;

    let mut executables = Vec::with_capacity(targets.len());
    for target in &targets {
        executables.push(build(&root, target)?);
    }

    let output = root.join(OUTPUT_DIR);
    if output.exists() {
        fs::remove_dir_all(&output)
            .map_err(|error| format!("cannot clear {}: {error}", output.display()))?;
    }
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;

    let mut sums = String::new();
    for (target, exe) in targets.iter().zip(&executables) {
        let name = zip_name(&version, target);
        let zip = output.join(&name);
        package(&root, exe, &zip)?;
        let bytes =
            fs::read(&zip).map_err(|error| format!("cannot read {}: {error}", zip.display()))?;
        sums.push_str(&sum_line(&sha256(&bytes)?, &name));
        println!("{}", zip.display());
    }
    let sums_path = output.join("SHA256SUMS.txt");
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

/// Gets the zip file name for a version and a target triple.
fn zip_name(version: &str, target: &str) -> String {
    format!("overlay-lock-{version}-{target}.zip")
}

/// Builds the release executable for `target` and returns its path, as Cargo reports it, so a
/// `CARGO_TARGET_DIR` or `build.target-dir` setting is followed.
fn build(root: &Path, target: &str) -> Result<PathBuf, String> {
    let output = Command::new(cargo())
        .current_dir(root)
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "overlay-lock",
            "--target",
            target,
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cannot start cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo build for {target} failed ({})",
            output.status
        ));
    }
    let messages = String::from_utf8_lossy(&output.stdout);
    let exe = messages
        .lines()
        .filter(|line| line.contains(r#""reason":"compiler-artifact""#))
        .filter_map(|line| json_string_field(line, "executable"))
        .find(|path| Path::new(path).file_name() == Some("overlay-lock.exe".as_ref()))
        .ok_or_else(|| format!("cargo build for {target} reported no overlay-lock.exe"))?;
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

/// Writes the zip: the executable, then the extra files from the repository root.
///
/// It uses the `tar.exe` that ships in System32 (Windows 10 1803 and later), which writes a zip
/// when the archive name ends in `.zip`. The one on `PATH` may be GNU tar, which cannot.
fn package(root: &Path, exe: &Path, zip: &Path) -> Result<(), String> {
    let (Some(folder), Some(name)) = (exe.parent(), exe.file_name()) else {
        return Err(format!("{} is not a file path", exe.display()));
    };
    if !exe.is_file() {
        return Err(format!("{} is missing", exe.display()));
    }
    let system_root = std::env::var_os("SystemRoot").ok_or("SystemRoot is not set")?;
    let tar = Path::new(&system_root).join("System32").join("tar.exe");
    let status = Command::new(&tar)
        .arg("-a")
        .arg("-c")
        .arg("-f")
        .arg(zip)
        .arg("-C")
        .arg(folder)
        .arg(name)
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
    use super::{json_string_field, sha256, sum_line, version_from_pkgid, zip_name};

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

    #[test]
    fn zip_name_matches_the_release_naming() {
        assert_eq!(
            zip_name("0.1.0", "x86_64-pc-windows-msvc"),
            "overlay-lock-0.1.0-x86_64-pc-windows-msvc.zip"
        );
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
