//! Embeds the application manifest, the icon and the version resource into `overlay-lock.exe`, and
//! records the build date for `--help` as `OVERLAY_LOCK_BUILD_DATE` (`YYYY-MM-DD`).

const MANIFEST: &str = "assets/overlay-lock.manifest";
const ICON: &str = "assets/overlay-lock.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
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

    let mut res = winresource::WindowsResource::new();
    res.set_manifest_file(MANIFEST)
        .set_icon(ICON)
        .set("ProductName", "OverlayLock")
        .set("FileDescription", "OverlayLock")
        .set("InternalName", "overlay-lock")
        .set("OriginalFilename", "overlay-lock.exe")
        .set("LegalCopyright", "Copyright (c) 2026 Joe Cox");
    if let Err(error) = res.compile() {
        panic!("failed to compile the Windows resources: {error}");
    }
}

/// Gets the build date: the UTC date of `SOURCE_DATE_EPOCH` when it is set (reproducible builds),
/// otherwise today's local date.
fn build_date() -> (u32, u32, u32) {
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
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
