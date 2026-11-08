//! Embeds the application manifest, the icon and the version resource into the executable, and
//! records the build date for `--help` as `OVERLAY_LOCK_BUILD_DATE` (`YYYY-MM-DD`).
//!
//! The version resource (Properties > Details) takes its values from `Cargo.toml` where it has
//! them: the version, the author as the company, the description, the licence and the repository.
//! The manifest's assembly identity gets the same version and the target's processor architecture.

use std::env;
use std::path::PathBuf;

// `Target`, `TARGETS` and `exe_name`, shared with `cargo xtask dist`'s zip names.
include!("release_targets.rs");

const MANIFEST: &str = "assets/overlay-lock.manifest";
const ICON: &str = "assets/overlay-lock.ico";

/// The product name shown in Properties > Details, and by Task Manager as the file description.
const PRODUCT: &str = "OverlayLock";

/// The first year of the copyright notice.
const FIRST_YEAR: u32 = 2026;

/// English (United Kingdom), the language of every string in the program.
const LANGUAGE_EN_GB: u16 = 0x0809;

/// `VS_FF_DEBUG`: the file has debugging information or was compiled with debugging enabled.
const VS_FF_DEBUG: u64 = 0x1;

/// `VS_FF_PRERELEASE`: the file is a development version, not a released one.
const VS_FF_PRERELEASE: u64 = 0x2;

/// The assembly identity in the manifest file, which the build fills in.
const MANIFEST_IDENTITY: &str =
    r#"<assemblyIdentity type="win32" name="BootBlock.OverlayLock" version="1.0.0.0"/>"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=release_targets.rs");
    println!("cargo:rerun-if-changed={MANIFEST}");
    println!("cargo:rerun-if-changed={ICON}");
    // The build date must change whenever the executable is rebuilt, so this script reruns on
    // every input of the crate, not only on its own files.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let (year, month, day) = build_date();
    println!("cargo:rustc-env=OVERLAY_LOCK_BUILD_DATE={year:04}-{month:02}-{day:02}");

    let triple = cargo_env("TARGET");
    let target = TARGETS.into_iter().find(|target| target.triple == triple);
    let author = cargo_env("CARGO_PKG_AUTHORS").replace(':', ", ");
    let built_for = target.map_or_else(
        || format!("Built for {triple}."),
        |target| format!("Built for {} ({triple}).", target.name),
    );
    let comments = format!(
        "{} {built_for} Released under the {} licence: {}",
        cargo_env("CARGO_PKG_DESCRIPTION"),
        cargo_env("CARGO_PKG_LICENSE"),
        cargo_env("CARGO_PKG_REPOSITORY"),
    );
    let mut flags = 0;
    if cargo_env("PROFILE") == "debug" {
        flags |= VS_FF_DEBUG;
    }
    if !cargo_env("CARGO_PKG_VERSION_PRE").is_empty() {
        flags |= VS_FF_PRERELEASE;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file(&manifest_file().to_string_lossy())
        .set_icon(ICON)
        .set_language(LANGUAGE_EN_GB)
        .set_version_info(winresource::VersionInfo::FILEFLAGS, flags)
        .set("CompanyName", &author)
        .set("ProductName", PRODUCT)
        .set("FileDescription", PRODUCT)
        .set("InternalName", "overlay-lock")
        .set(
            "OriginalFilename",
            &target.map_or_else(|| "overlay-lock.exe".to_string(), exe_name),
        )
        .set("LegalCopyright", &copyright(&author, year))
        .set("Comments", &comments);
    if let Err(error) = res.compile() {
        panic!("failed to compile the Windows resources: {error}");
    }
}

/// Gets a variable Cargo sets for build scripts.
fn cargo_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Cargo sets {name} for build scripts"))
}

/// Gets the copyright notice, with a year range once the build year is past the first year.
fn copyright(author: &str, year: u32) -> String {
    if year > FIRST_YEAR {
        format!("Copyright \u{a9} {FIRST_YEAR}\u{2013}{year} {author}")
    } else {
        format!("Copyright \u{a9} {FIRST_YEAR} {author}")
    }
}

/// Writes the manifest with its assembly identity filled in (the package version as
/// `major.minor.patch.0`, and the target's processor architecture) into `OUT_DIR`, and returns its
/// path.
fn manifest_file() -> PathBuf {
    let text = std::fs::read_to_string(MANIFEST)
        .unwrap_or_else(|error| panic!("cannot read {MANIFEST}: {error}"));
    assert_eq!(
        text.matches(MANIFEST_IDENTITY).count(),
        1,
        "{MANIFEST} must hold the assembly identity line exactly once"
    );
    let version = format!(
        "{}.{}.{}.0",
        cargo_env("CARGO_PKG_VERSION_MAJOR"),
        cargo_env("CARGO_PKG_VERSION_MINOR"),
        cargo_env("CARGO_PKG_VERSION_PATCH"),
    );
    let architecture = match cargo_env("CARGO_CFG_TARGET_ARCH").as_str() {
        "x86_64" => r#" processorArchitecture="amd64""#,
        "x86" => r#" processorArchitecture="x86""#,
        "aarch64" => r#" processorArchitecture="arm64""#,
        _ => "",
    };
    let identity = format!(
        r#"<assemblyIdentity type="win32" name="BootBlock.OverlayLock" version="{version}"{architecture}/>"#
    );
    let path = PathBuf::from(cargo_env("OUT_DIR")).join("overlay-lock.manifest");
    std::fs::write(&path, text.replace(MANIFEST_IDENTITY, &identity))
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
    path
}

/// Gets the build date: the UTC date of `SOURCE_DATE_EPOCH` when it is set (reproducible builds),
/// otherwise today's local date.
fn build_date() -> (u32, u32, u32) {
    if let Ok(epoch) = env::var("SOURCE_DATE_EPOCH") {
        let seconds: i64 = epoch
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("SOURCE_DATE_EPOCH is not a whole number: {epoch:?}"));
        return civil_from_days(seconds.div_euclid(86_400));
    }
    local_date()
}

/// Converts days since 1970-01-01 to a (year, month, day) date in the proleptic Gregorian calendar
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (u32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let narrow = |value: i64| u32::try_from(value).expect("the date is after year 0");
    (narrow(year), narrow(month), narrow(day))
}

/// The Win32 `SYSTEMTIME` structure.
#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(time: *mut SystemTime);
}

/// Gets today's date in the local time zone of the machine that builds.
fn local_date() -> (u32, u32, u32) {
    let mut time = SystemTime::default();
    // SAFETY: `GetLocalTime` fills the `SYSTEMTIME` it is given and has no other preconditions.
    unsafe { GetLocalTime(&raw mut time) };
    (
        u32::from(time.year),
        u32::from(time.month),
        u32::from(time.day),
    )
}
