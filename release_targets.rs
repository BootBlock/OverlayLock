// The release targets, shared by `build.rs` (the version resource's original file name) and
// `cargo xtask dist` (the zip and executable names) through `include!`, so the two cannot drift.

/// A target a release ships: the short CPU name its file names use, and the Rust target triple it
/// is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    name: &'static str,
    triple: &'static str,
}

/// The targets a release ships, in the order they are built. Each triple also needs its static
/// C runtime table in `.cargo/config.toml`.
const TARGETS: [Target; 3] = [
    Target {
        name: "x64",
        triple: "x86_64-pc-windows-msvc",
    },
    Target {
        name: "x86",
        triple: "i686-pc-windows-msvc",
    },
    Target {
        name: "arm64",
        triple: "aarch64-pc-windows-msvc",
    },
];

/// Gets the file name of a target's executable in its release zip, such as
/// `overlay-lock-x64.exe`.
fn exe_name(target: Target) -> String {
    format!("overlay-lock-{}.exe", target.name)
}
