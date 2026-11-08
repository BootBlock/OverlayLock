//! Parses the `--timeout` duration: one or more `<whole number><unit>` groups with units `h`, `m`
//! and `s`, in that order, each at most once, such as `90s`, `30m` or `1h30m`.

use std::fmt;
use std::time::Duration;

/// The shortest duration allowed.
const MIN: Duration = Duration::from_secs(1);

/// The longest duration allowed: 7 days.
const MAX: Duration = Duration::from_hours(7 * 24);

/// The reason a duration did not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurationError {
    /// The text is not `<number><unit>` groups in `h`, `m`, `s` order.
    Format(String),
    /// The total is shorter than 1 second or longer than 7 days.
    OutOfRange(String),
}

impl fmt::Display for DurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(text) => write!(
                f,
                "'{text}' is not a duration: use whole numbers with h, m and s in that order, such \
                 as 90s, 30m or 1h30m"
            ),
            Self::OutOfRange(text) => write!(f, "{text} is outside 1s to 7 days"),
        }
    }
}

impl std::error::Error for DurationError {}

/// Parses a duration such as `90s`, `30m`, `1h30m` or `2h`.
pub fn parse_duration(text: &str) -> Result<Duration, DurationError> {
    let format_error = || DurationError::Format(text.to_owned());
    let units = [('h', 3600u64), ('m', 60), ('s', 1)];
    let mut next_unit = 0;
    let mut total: u64 = 0;
    let mut rest = text;
    if rest.is_empty() {
        return Err(format_error());
    }
    while !rest.is_empty() {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return Err(format_error());
        }
        let (number, tail) = rest.split_at(digits);
        let unit = tail.chars().next().ok_or_else(format_error)?;
        let offset = units[next_unit..]
            .iter()
            .position(|&(name, _)| name == unit)
            .ok_or_else(format_error)?;
        let seconds_per_unit = units[next_unit + offset].1;
        next_unit += offset + 1;
        // A number too large for u64 is far outside the range, not a format error.
        let value: u64 = number
            .parse()
            .map_err(|_| DurationError::OutOfRange(text.to_owned()))?;
        total = value
            .checked_mul(seconds_per_unit)
            .and_then(|seconds| total.checked_add(seconds))
            .ok_or_else(|| DurationError::OutOfRange(text.to_owned()))?;
        rest = &tail[unit.len_utf8()..];
    }
    let duration = Duration::from_secs(total);
    if (MIN..=MAX).contains(&duration) {
        Ok(duration)
    } else {
        Err(DurationError::OutOfRange(text.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::{DurationError, parse_duration};
    use std::time::Duration;

    #[test]
    fn valid_durations() {
        let cases = [
            ("90s", 90),
            ("30m", 1800),
            ("1h30m", 5400),
            ("2h", 7200),
            ("1h1m1s", 3661),
            ("1m30s", 90),
            ("1s", 1),
            ("168h", 604_800),
            ("0h1s", 1),
        ];
        for (text, seconds) in cases {
            assert_eq!(
                parse_duration(text),
                Ok(Duration::from_secs(seconds)),
                "{text}"
            );
        }
    }

    #[test]
    fn badly_formed_durations() {
        for text in [
            "", "30", "1m1h", "1.5h", "1h1h", "h", "1 h", "1H", "1d", "-1s", "1s ", "s1",
        ] {
            assert!(
                matches!(parse_duration(text), Err(DurationError::Format(_))),
                "{text:?} should be a format error"
            );
        }
    }

    #[test]
    fn out_of_range_durations() {
        for text in [
            "0s",
            "0h0m0s",
            "168h1s",
            "605000s",
            "99999999999999999999999h",
        ] {
            assert!(
                matches!(parse_duration(text), Err(DurationError::OutOfRange(_))),
                "{text:?} should be out of range"
            );
        }
    }
}
