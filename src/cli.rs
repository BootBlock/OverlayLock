//! Parses the command line into [`Options`].

use std::ffi::OsString;
use std::fmt;
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;

use lexopt::{Arg, Parser, ValueExt};

use crate::colour::Colour;
use crate::duration::parse_duration;
use crate::monitors::Monitors;

/// The build date `build.rs` recorded, as `YYYY-MM-DD`.
const BUILD_DATE: &str = env!("OVERLAY_LOCK_BUILD_DATE");

/// The month names, January first.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Gets the help text: a first line with the version and the build date, then [`HELP_BODY`].
pub fn help() -> String {
    format!(
        "OverlayLock {}, built on the {}\n{HELP_BODY}",
        env!("CARGO_PKG_VERSION"),
        long_date(BUILD_DATE)
    )
}

/// Formats a `YYYY-MM-DD` date as an ordinal day, the month name and the year, such as
/// `8th November 2026`. Gives the text back unchanged if it is not such a date.
fn long_date(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    let parsed = match parts.as_slice() {
        [year, month, day] if year.len() == 4 && month.len() == 2 && day.len() == 2 => year
            .parse::<u32>()
            .ok()
            .zip(month.parse::<usize>().ok())
            .zip(day.parse::<u32>().ok())
            .map(|((year, month), day)| (year, month, day)),
        _ => None,
    };
    match parsed {
        Some((year, month @ 1..=12, day @ 1..=31)) => {
            let suffix = match (day % 10, day % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{day}{suffix} {} {year}", MONTHS[month - 1])
        }
        _ => iso.to_string(),
    }
}

/// The help text after its first line: the only list of the options in prose. The README copies
/// the whole help exactly, apart from the build date, so a new version in `Cargo.toml` needs the
/// README's copy updated too (`tests/readme.rs` checks).
pub const HELP_BODY: &str = "\
Created by Joe Cox - https://bootblock.co.uk

OverlayLock covers the screen until a password is typed.

Usage: overlay-lock --password <TEXT> [OPTIONS]

Options:
  -p, --password <TEXT>       The password that unlocks. Required, not empty, case-sensitive.
      --monitors <all|cursor> Cover every monitor, or only the one under the cursor.
                              [default: all]
      --colour <COLOUR>       The background colour: #RGB, #RRGGBB or a CSS colour name.
                              Alias: --color. [default: black]
      --message <TEXT>        Text to show in the centre. \\n starts a new line, \\\\ is a
                              backslash.
      --text-colour <COLOUR>  The message colour. Alias: --text-color. [default: #CCCCCC]
      --text-size <POINTS>    The message size in points, 4 to 400. [default: 28]
      --font <FAMILY>         The message font family.
                              [default: Segoe UI Variable Display, else Segoe UI]
      --block-keys            Swallow every key system-wide while locked.
      --block-mouse           Swallow every mouse event system-wide while locked.
      --timeout <DURATION>    Unlock after this long, such as 90s, 30m or 1h30m (1s to 7 days).
      --ignore-alt-f4         Alt+F4 does not unlock.
      --yield-on-ctrl-alt-del After the Ctrl+Alt+Del screen, step back so Task Manager gets
                              input. The password and the timeout still unlock.
  -h, --help                  Print this help and exit.
  -V, --version               Print the version and exit.

The --text-colour, --text-size and --font options need --message.

Exit codes:
  0  Unlocked (or help or version shown)
  1  A runtime failure
  2  A usage error
  3  Another instance is already locked";

/// The smallest and largest `--text-size`, in points.
const TEXT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 4.0..=400.0;

/// The default message size, in points.
const DEFAULT_TEXT_SIZE: f32 = 28.0;

/// The default background colour.
const DEFAULT_BACKGROUND: Colour = Colour::hex(0x00_0000);

/// The default message colour.
const DEFAULT_TEXT_COLOUR: Colour = Colour::hex(0xCC_CCCC);

/// What the command line asks for.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// Lock with these options.
    Run(Options),
    /// Print the help text.
    Help,
    /// Print the version.
    Version,
}

/// The options of a lock.
#[derive(Debug, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one independent command-line flag"
)]
pub struct Options {
    /// The password as UTF-16 code units, never empty.
    pub password: Vec<u16>,
    /// Which monitors to cover.
    pub monitors: Monitors,
    /// The background colour.
    pub background: Colour,
    /// The centred message, if any.
    pub message: Option<MessageStyle>,
    /// `true` to install the keyboard hook; otherwise, `false`.
    pub block_keys: bool,
    /// `true` to install the mouse hook; otherwise, `false`.
    pub block_mouse: bool,
    /// The time after which the lock ends by itself, if any.
    pub timeout: Option<Duration>,
    /// `true` if Alt+F4 unlocks; otherwise, `false`.
    pub alt_f4_unlocks: bool,
    /// `true` to step back after the Ctrl+Alt+Del screen; otherwise, `false`.
    pub yield_on_ctrl_alt_del: bool,
}

/// The centred message and how it looks.
#[derive(Debug, PartialEq)]
pub struct MessageStyle {
    /// The text, with its escapes decoded.
    pub text: String,
    /// The text colour.
    pub colour: Colour,
    /// The text size in points.
    pub size_points: f32,
    /// The font family, or `None` for the default chain.
    pub font: Option<String>,
}

/// A command line that is not valid. Its text names the option and the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(String);

impl UsageError {
    fn new(option: &str, reason: impl fmt::Display) -> Self {
        Self(format!("{option}: {reason}"))
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

impl From<lexopt::Error> for UsageError {
    fn from(error: lexopt::Error) -> Self {
        Self(error.to_string())
    }
}

/// Parses the arguments that follow the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut parser = Parser::from_args(args);
    let mut builder = Builder::default();
    while let Some(arg) = parser.next()? {
        match arg {
            Arg::Short('h') | Arg::Long("help") => return Ok(Command::Help),
            Arg::Short('V') | Arg::Long("version") => return Ok(Command::Version),
            Arg::Value(value) => {
                return Err(UsageError(format!(
                    "unexpected argument '{}': every value follows an option",
                    value.to_string_lossy()
                )));
            }
            option => {
                let name = option_name(&option);
                builder.apply(&name, &mut parser)?;
            }
        }
    }
    builder.finish().map(Command::Run)
}

/// The options seen so far, each `None` or `false` until given.
#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each field is one independent command-line flag"
)]
struct Builder {
    password: Option<Vec<u16>>,
    monitors: Option<Monitors>,
    background: Option<Colour>,
    message: Option<String>,
    text_colour: Option<Colour>,
    text_size: Option<f32>,
    font: Option<String>,
    timeout: Option<Duration>,
    block_keys: bool,
    block_mouse: bool,
    ignore_alt_f4: bool,
    yield_on_ctrl_alt_del: bool,
}

impl Builder {
    /// Reads one option, named as the user typed it, and its value if it takes one.
    fn apply(&mut self, name: &str, parser: &mut Parser) -> Result<(), UsageError> {
        match name {
            "-p" | "--password" => {
                let value: Vec<u16> = parser.value()?.encode_wide().collect();
                if value.is_empty() {
                    return Err(UsageError::new(name, "the password must not be empty"));
                }
                set_once(&mut self.password, value, name)
            }
            "--monitors" => {
                let value = parser.value()?.string()?;
                let mode = match value.to_ascii_lowercase().as_str() {
                    "all" => Monitors::All,
                    "cursor" => Monitors::Cursor,
                    _ => {
                        let reason = format!("'{value}' is not all or cursor");
                        return Err(UsageError::new(name, reason));
                    }
                };
                set_once(&mut self.monitors, mode, name)
            }
            "--colour" | "--color" => {
                let value = parse_colour(parser, name)?;
                set_once(&mut self.background, value, name)
            }
            "--message" => {
                let value = parser.value()?.string()?;
                let text =
                    decode_escapes(&value).map_err(|reason| UsageError::new(name, reason))?;
                set_once(&mut self.message, text, name)
            }
            "--text-colour" | "--text-color" => {
                let value = parse_colour(parser, name)?;
                set_once(&mut self.text_colour, value, name)
            }
            "--text-size" => {
                let value = parser.value()?.string()?;
                let size =
                    parse_text_size(&value).map_err(|reason| UsageError::new(name, reason))?;
                set_once(&mut self.text_size, size, name)
            }
            "--font" => {
                let value = parser.value()?.string()?;
                if value.trim().is_empty() {
                    return Err(UsageError::new(name, "the font family must not be empty"));
                }
                set_once(&mut self.font, value, name)
            }
            "--timeout" => {
                let value = parser.value()?.string()?;
                let timeout =
                    parse_duration(&value).map_err(|reason| UsageError::new(name, reason))?;
                set_once(&mut self.timeout, timeout, name)
            }
            "--block-keys" => set_flag(&mut self.block_keys, name),
            "--block-mouse" => set_flag(&mut self.block_mouse, name),
            "--ignore-alt-f4" => set_flag(&mut self.ignore_alt_f4, name),
            "--yield-on-ctrl-alt-del" => set_flag(&mut self.yield_on_ctrl_alt_del, name),
            _ => Err(UsageError::new(name, "unknown option")),
        }
    }

    /// Checks the options together and fills in the defaults.
    fn finish(self) -> Result<Options, UsageError> {
        let password = self
            .password
            .ok_or_else(|| UsageError::new("--password", "the password is required"))?;
        let message = if let Some(text) = self.message {
            Some(MessageStyle {
                text,
                colour: self.text_colour.unwrap_or(DEFAULT_TEXT_COLOUR),
                size_points: self.text_size.unwrap_or(DEFAULT_TEXT_SIZE),
                font: self.font,
            })
        } else {
            let orphan = [
                ("--text-colour", self.text_colour.is_some()),
                ("--text-size", self.text_size.is_some()),
                ("--font", self.font.is_some()),
            ]
            .into_iter()
            .find_map(|(option, given)| given.then_some(option));
            if let Some(option) = orphan {
                return Err(UsageError::new(option, "needs --message"));
            }
            None
        };
        Ok(Options {
            password,
            monitors: self.monitors.unwrap_or_default(),
            background: self.background.unwrap_or(DEFAULT_BACKGROUND),
            message,
            block_keys: self.block_keys,
            block_mouse: self.block_mouse,
            timeout: self.timeout,
            alt_f4_unlocks: !self.ignore_alt_f4,
            yield_on_ctrl_alt_del: self.yield_on_ctrl_alt_del,
        })
    }
}

/// Gets the option as the user typed it, such as `-p` or `--color`.
fn option_name(arg: &Arg<'_>) -> String {
    match arg {
        Arg::Short(short) => format!("-{short}"),
        Arg::Long(long) => format!("--{long}"),
        Arg::Value(_) => String::new(),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError::new(name, "given more than once"));
    }
    *slot = Some(value);
    Ok(())
}

fn set_flag(flag: &mut bool, name: &str) -> Result<(), UsageError> {
    if *flag {
        return Err(UsageError::new(name, "given more than once"));
    }
    *flag = true;
    Ok(())
}

fn parse_colour(parser: &mut Parser, name: &str) -> Result<Colour, UsageError> {
    let value = parser.value()?.string()?;
    value
        .parse()
        .map_err(|reason| UsageError::new(name, reason))
}

/// Parses a size in points: digits with an optional decimal part, 4 to 400.
fn parse_text_size(text: &str) -> Result<f32, String> {
    let well_formed = {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
        !whole.is_empty()
            && !fraction.is_empty()
            && whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
    };
    let size: f32 = text
        .parse()
        .ok()
        .filter(|_| well_formed)
        .ok_or_else(|| format!("'{text}' is not a number of points"))?;
    if TEXT_SIZE_RANGE.contains(&size) {
        Ok(size)
    } else {
        Err(format!("{text} is outside 4 to 400"))
    }
}

/// Decodes `\n` to a line break and `\\` to a backslash. Any other escape is an error.
fn decode_escapes(text: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            decoded.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => decoded.push('\n'),
            Some('\\') => decoded.push('\\'),
            Some(other) => {
                return Err(format!(
                    "'\\{other}' is not an escape: use \\n for a new line or \\\\ for a backslash"
                ));
            }
            None => {
                return Err(
                    "the text ends with a lone backslash: use \\\\ for a backslash".to_owned(),
                );
            }
        }
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{Command, MessageStyle, Options, UsageError, parse};
    use crate::colour::Colour;
    use crate::monitors::Monitors;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::time::Duration;

    fn run(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    fn options(args: &[&str]) -> Options {
        match run(args) {
            Ok(Command::Run(options)) => options,
            other => panic!("{args:?} gave {other:?}"),
        }
    }

    fn error(args: &[&str]) -> String {
        match run(args) {
            Err(error) => error.to_string(),
            other => panic!("{args:?} should fail but gave {other:?}"),
        }
    }

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn defaults() {
        assert_eq!(
            options(&["-p", "go"]),
            Options {
                password: utf16("go"),
                monitors: Monitors::All,
                background: Colour::hex(0x00_0000),
                message: None,
                block_keys: false,
                block_mouse: false,
                timeout: None,
                alt_f4_unlocks: true,
                yield_on_ctrl_alt_del: false,
            }
        );
    }

    #[test]
    fn every_option() {
        let parsed = options(&[
            "--password",
            "Secret 1",
            "--monitors",
            "cursor",
            "--colour",
            "#102030",
            "--message",
            "Locked\\nBack soon \\\\o/",
            "--text-colour",
            "gold",
            "--text-size",
            "48.5",
            "--font",
            "Comic Sans MS",
            "--block-keys",
            "--block-mouse",
            "--timeout",
            "1h30m",
            "--ignore-alt-f4",
            "--yield-on-ctrl-alt-del",
        ]);
        assert_eq!(
            parsed,
            Options {
                password: utf16("Secret 1"),
                monitors: Monitors::Cursor,
                background: Colour::hex(0x10_2030),
                message: Some(MessageStyle {
                    text: "Locked\nBack soon \\o/".to_owned(),
                    colour: Colour::hex(0xFF_D700),
                    size_points: 48.5,
                    font: Some("Comic Sans MS".to_owned()),
                }),
                block_keys: true,
                block_mouse: true,
                timeout: Some(Duration::from_mins(90)),
                alt_f4_unlocks: false,
                yield_on_ctrl_alt_del: true,
            }
        );
    }

    #[test]
    fn message_defaults() {
        let message = options(&["-p", "go", "--message", "Hi"]).message.unwrap();
        assert_eq!(message.colour, Colour::hex(0xCC_CCCC));
        assert!((message.size_points - 28.0).abs() < f32::EPSILON);
        assert_eq!(message.font, None);
    }

    #[test]
    fn american_aliases() {
        let parsed = options(&[
            "-p",
            "go",
            "--color",
            "navy",
            "--message",
            "m",
            "--text-color",
            "#fff",
        ]);
        assert_eq!(parsed.background, Colour::hex(0x00_0080));
        assert_eq!(parsed.message.unwrap().colour, Colour::hex(0xFF_FFFF));
    }

    #[test]
    fn alias_and_option_together_are_a_duplicate() {
        assert!(
            error(&["-p", "go", "--colour", "red", "--color", "red"]).contains("more than once")
        );
    }

    #[test]
    fn equals_form_and_short_attached_form() {
        assert_eq!(options(&["--password=go"]).password, utf16("go"));
        assert_eq!(options(&["-pgo"]).password, utf16("go"));
    }

    #[test]
    fn password_that_is_not_unicode_round_trips() {
        // A lone surrogate is valid in a Windows command line but not in a Rust string.
        let units = vec![0xD800, u16::from(b'x')];
        let args = [OsString::from("-p"), OsString::from_wide(&units)];
        match parse(args) {
            Ok(Command::Run(options)) => assert_eq!(options.password, units),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn help_and_version() {
        assert_eq!(run(&["--help"]), Ok(Command::Help));
        assert_eq!(run(&["-h"]), Ok(Command::Help));
        assert_eq!(run(&["--version"]), Ok(Command::Version));
        assert_eq!(run(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn missing_password() {
        assert!(error(&[]).starts_with("--password:"));
        assert!(error(&["--block-keys"]).starts_with("--password:"));
    }

    #[test]
    fn empty_password() {
        assert!(error(&["-p", ""]).contains("must not be empty"));
        assert!(error(&["--password="]).contains("must not be empty"));
    }

    #[test]
    fn password_without_value() {
        assert!(error(&["--password"]).contains("--password"));
    }

    #[test]
    fn duplicates() {
        for args in [
            &["-p", "a", "--password", "b"][..],
            &["-p", "a", "--block-keys", "--block-keys"],
            &["-p", "a", "--timeout", "1s", "--timeout", "2s"],
            &["-p", "a", "--monitors", "all", "--monitors", "all"],
        ] {
            assert!(error(args).contains("given more than once"), "{args:?}");
        }
    }

    #[test]
    fn unknown_option() {
        assert_eq!(
            error(&["-p", "go", "--blockkeys"]),
            "--blockkeys: unknown option"
        );
        assert_eq!(error(&["-p", "go", "-x"]), "-x: unknown option");
    }

    #[test]
    fn flag_with_a_value() {
        assert!(error(&["-p", "go", "--block-keys=yes"]).contains("--block-keys"));
    }

    #[test]
    fn positional_argument() {
        assert!(error(&["-p", "go", "stray"]).contains("'stray'"));
    }

    #[test]
    fn out_of_range_values() {
        assert_eq!(
            error(&["-p", "go", "--message", "m", "--text-size", "500"]),
            "--text-size: 500 is outside 4 to 400"
        );
        assert!(error(&["-p", "go", "--message", "m", "--text-size", "3.9"]).contains("outside"));
        for size in ["abc", "1e2", "inf", "NaN", ".5", "5.", "-5"] {
            assert!(
                error(&["-p", "go", "--message", "m", "--text-size", size])
                    .contains("not a number"),
                "{size}"
            );
        }
        assert!(error(&["-p", "go", "--timeout", "0s"]).starts_with("--timeout:"));
        assert!(error(&["-p", "go", "--timeout", "8d"]).starts_with("--timeout:"));
        assert!(error(&["-p", "go", "--monitors", "left"]).starts_with("--monitors:"));
        assert!(error(&["-p", "go", "--colour", "#12345"]).starts_with("--colour:"));
        assert!(error(&["-p", "go", "--color", "nope"]).starts_with("--color:"));
    }

    #[test]
    fn text_size_bounds_are_inclusive() {
        for size in ["4", "400", "4.0", "399.99"] {
            let args = ["-p", "go", "--message", "m", "--text-size", size];
            assert!(options(&args).message.is_some(), "{size}");
        }
    }

    #[test]
    fn message_options_need_message() {
        assert_eq!(
            error(&["-p", "go", "--text-colour", "red"]),
            "--text-colour: needs --message"
        );
        assert_eq!(
            error(&["-p", "go", "--text-size", "40"]),
            "--text-size: needs --message"
        );
        assert_eq!(
            error(&["-p", "go", "--font", "Arial"]),
            "--font: needs --message"
        );
    }

    #[test]
    fn bad_escapes() {
        assert!(error(&["-p", "go", "--message", "a\\tb"]).starts_with("--message: '\\t'"));
        assert!(error(&["-p", "go", "--message", "end\\"]).contains("lone backslash"));
    }

    #[test]
    fn monitors_is_case_insensitive() {
        assert_eq!(
            options(&["-p", "go", "--monitors", "Cursor"]).monitors,
            Monitors::Cursor
        );
    }

    #[test]
    fn help_lists_every_option() {
        for option in [
            "--password",
            "--monitors",
            "--colour",
            "--color",
            "--message",
            "--text-colour",
            "--text-color",
            "--text-size",
            "--font",
            "--block-keys",
            "--block-mouse",
            "--timeout",
            "--ignore-alt-f4",
            "--yield-on-ctrl-alt-del",
            "--help",
            "--version",
        ] {
            assert!(super::help().contains(option), "{option}");
        }
    }

    #[test]
    fn help_starts_with_the_name_version_build_date_and_author() {
        let help = super::help();
        let mut lines = help.lines();
        let first = format!(
            "OverlayLock {}, built on the {}",
            env!("CARGO_PKG_VERSION"),
            super::long_date(super::BUILD_DATE)
        );
        assert_eq!(lines.next(), Some(first.as_str()));
        assert_eq!(
            lines.next(),
            Some("Created by Joe Cox - https://bootblock.co.uk")
        );
        assert_eq!(lines.next(), Some(""));
    }

    #[test]
    fn long_date_uses_english_ordinals() {
        for (iso, long) in [
            ("2026-11-01", "1st November 2026"),
            ("2026-11-02", "2nd November 2026"),
            ("2026-11-03", "3rd November 2026"),
            ("2026-11-08", "8th November 2026"),
            ("2026-11-11", "11th November 2026"),
            ("2026-11-12", "12th November 2026"),
            ("2026-11-13", "13th November 2026"),
            ("2026-11-21", "21st November 2026"),
            ("2026-11-22", "22nd November 2026"),
            ("2026-11-23", "23rd November 2026"),
            ("2026-01-31", "31st January 2026"),
            ("2027-12-30", "30th December 2027"),
        ] {
            assert_eq!(super::long_date(iso), long, "{iso}");
        }
    }

    #[test]
    fn long_date_gives_back_text_that_is_not_a_date() {
        for text in [
            "",
            "2026-11",
            "2026-13-01",
            "2026-00-10",
            "2026-11-32",
            "26-11-08",
            "x",
        ] {
            assert_eq!(super::long_date(text), text);
        }
    }

    /// The date `build.rs` recorded is a real date, so the help never shows the raw text.
    #[test]
    fn the_recorded_build_date_formats() {
        assert_ne!(super::long_date(super::BUILD_DATE), super::BUILD_DATE);
    }
}
