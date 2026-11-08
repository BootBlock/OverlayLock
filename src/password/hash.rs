//! The password hash that `--password-hash` takes and `--make-hash` prints.
//!
//! The text is `pbkdf2-sha256:<iterations>:<length>:<salt>:<hash>`: PBKDF2-HMAC-SHA256 of the
//! password's UTF-16 code units, little-endian, with `<iterations>` rounds; `<length>` is the
//! password's length in UTF-16 code units, which the matcher needs (OL6), so the text reveals the
//! length; `<salt>` and `<hash>` are unpadded standard Base64. A colon separates the fields
//! because, unlike `$`, no shell expands it inside double quotes.

use std::fmt;
use std::str::FromStr;

use super::base64;
use super::pbkdf2::{Pbkdf2, fill_random};

/// The scheme name that starts the text.
const SCHEME: &str = "pbkdf2-sha256";

/// The rounds `--make-hash` uses.
///
/// The match thread hashes once per typed unit after the first N, so a hash must take less time
/// than the gap between two units, or input queues up and the unlock lags. A held key repeats up to
/// about 30 times a second, which leaves 33 ms per hash. On 2026-11-08, on the owner's PC, one
/// hash of 10,000 rounds took 1.9 to 3.9 ms in the x64 build and 4.1 ms in the x86 build, and the
/// time grows in step with the rounds, so 5,000 rounds leave a processor ten times slower room to
/// keep up. The threat is a glance at a command line, not an offline attack on the hash, so a low
/// count costs little.
pub const DEFAULT_ITERATIONS: u32 = 5_000;

/// The fewest and most rounds a hash text may give.
pub const ITERATION_RANGE: std::ops::RangeInclusive<u32> = 1_000..=100_000;

/// The longest password a hash may stand for, in UTF-16 code units.
pub const MAX_LENGTH: usize = 1_024;

/// The salt length `--make-hash` uses, in bytes.
const SALT_LENGTH: usize = 16;

/// The shortest and longest salt a hash text may give, in bytes.
const SALT_RANGE: std::ops::RangeInclusive<usize> = 16..=64;

/// The hash length, in bytes: one SHA-256 output.
pub const HASH_LENGTH: usize = 32;

/// A parsed password hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordHash {
    iterations: u32,
    length: usize,
    salt: Box<[u8]>,
    hash: [u8; HASH_LENGTH],
}

impl PasswordHash {
    /// Hashes `password` with a new random salt and [`DEFAULT_ITERATIONS`].
    ///
    /// The caller checks that the password is not empty and at most [`MAX_LENGTH`] units long.
    pub fn make(password: &[u16]) -> windows::core::Result<Self> {
        debug_assert!(
            !password.is_empty() && password.len() <= MAX_LENGTH,
            "the caller checks the password length"
        );
        let mut salt = vec![0u8; SALT_LENGTH].into_boxed_slice();
        fill_random(&mut salt)?;
        let mut hash = [0u8; HASH_LENGTH];
        Pbkdf2::open()?.derive(
            &utf16_le_bytes(password),
            &salt,
            DEFAULT_ITERATIONS,
            &mut hash,
        )?;
        Ok(Self {
            iterations: DEFAULT_ITERATIONS,
            length: password.len(),
            salt,
            hash,
        })
    }

    /// Gets the password length in UTF-16 code units.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Returns `true` if `password`, the UTF-16 code units as little-endian bytes, hashes to this
    /// hash; otherwise, `false`. A derivation that fails never matches.
    ///
    /// The final comparison takes the same time wherever the hashes differ.
    pub fn matches(&self, pbkdf2: &Pbkdf2, password: &[u8]) -> bool {
        let mut derived = [0u8; HASH_LENGTH];
        pbkdf2
            .derive(password, &self.salt, self.iterations, &mut derived)
            .is_ok()
            && derived
                .iter()
                .zip(&self.hash)
                .fold(0u8, |difference, (a, b)| difference | (a ^ b))
                == 0
    }
}

/// Writes UTF-16 code units as little-endian bytes into a new buffer.
pub fn utf16_le_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
}

impl fmt::Display for PasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{SCHEME}:{}:{}:{}:{}",
            self.iterations,
            self.length,
            base64::encode(&self.salt),
            base64::encode(&self.hash)
        )
    }
}

impl FromStr for PasswordHash {
    type Err = String;

    /// Parses the text `--make-hash` prints. The error says which part is wrong.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let fields: Vec<&str> = text.split(':').collect();
        let [scheme, iterations, length, salt, hash] = fields.as_slice() else {
            return Err(format!(
                "not a password hash: expected {SCHEME}:<iterations>:<length>:<salt>:<hash>, \
                 as overlay-lock --make-hash prints"
            ));
        };
        if *scheme != SCHEME {
            return Err(format!("'{scheme}' is not a known scheme: use {SCHEME}"));
        }
        let iterations = parse_number(iterations)
            .and_then(|number| u32::try_from(number).ok())
            .filter(|number| ITERATION_RANGE.contains(number))
            .ok_or_else(|| {
                format!(
                    "the iteration count '{iterations}' is not a whole number from {} to {}",
                    ITERATION_RANGE.start(),
                    ITERATION_RANGE.end()
                )
            })?;
        let length = parse_number(length)
            .filter(|number| (1..=MAX_LENGTH).contains(number))
            .ok_or_else(|| {
                format!("the length '{length}' is not a whole number from 1 to {MAX_LENGTH}")
            })?;
        let salt = base64::decode(salt)
            .filter(|bytes| SALT_RANGE.contains(&bytes.len()))
            .ok_or_else(|| {
                format!(
                    "the salt is not Base64 of {} to {} bytes",
                    SALT_RANGE.start(),
                    SALT_RANGE.end()
                )
            })?
            .into_boxed_slice();
        let hash = base64::decode(hash)
            .and_then(|bytes| <[u8; HASH_LENGTH]>::try_from(bytes).ok())
            .ok_or_else(|| format!("the hash is not Base64 of {HASH_LENGTH} bytes"))?;
        Ok(Self {
            iterations,
            length,
            salt,
            hash,
        })
    }
}

/// Parses a decimal number of ASCII digits with no sign and no leading zero.
fn parse_number(text: &str) -> Option<usize> {
    let canonical = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_ITERATIONS, ITERATION_RANGE, PasswordHash, utf16_le_bytes};
    use crate::password::pbkdf2::Pbkdf2;

    /// The hash of "go", made by `--make-hash`'s code path with a fixed salt.
    fn known() -> String {
        let salt = super::base64::encode(&[7u8; 16]);
        let mut hash = [0u8; 32];
        Pbkdf2::open()
            .unwrap()
            .derive(&utf16_le_bytes(&utf16("go")), &[7u8; 16], 1_000, &mut hash)
            .unwrap();
        format!(
            "pbkdf2-sha256:1000:2:{salt}:{}",
            super::base64::encode(&hash)
        )
    }

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn matches(hash: &PasswordHash, password: &str) -> bool {
        hash.matches(&Pbkdf2::open().unwrap(), &utf16_le_bytes(&utf16(password)))
    }

    #[test]
    fn made_hash_round_trips_through_its_text_and_matches_only_its_password() {
        let made = PasswordHash::make(&utf16("Purr 42")).expect("the hash is made");
        let text = made.to_string();
        assert!(
            text.starts_with(&format!("pbkdf2-sha256:{DEFAULT_ITERATIONS}:7:")),
            "{text}"
        );
        let parsed: PasswordHash = text.parse().expect("the text parses");
        assert_eq!(parsed, made);
        assert_eq!(parsed.length(), 7);
        assert!(matches(&parsed, "Purr 42"));
        assert!(!matches(&parsed, "purr 42"));
        assert!(!matches(&parsed, "Purr 43"));
    }

    #[test]
    fn two_hashes_of_one_password_differ_by_salt() {
        let first = PasswordHash::make(&utf16("go")).unwrap();
        let second = PasswordHash::make(&utf16("go")).unwrap();
        assert_ne!(first.to_string(), second.to_string());
    }

    #[test]
    fn default_iterations_are_in_the_accepted_range() {
        assert!(ITERATION_RANGE.contains(&DEFAULT_ITERATIONS));
    }

    #[test]
    fn parses_a_known_hash() {
        let hash: PasswordHash = known().parse().expect("the text parses");
        assert!(matches(&hash, "go"));
        assert!(!matches(&hash, "og"));
    }

    #[test]
    fn rejects_malformed_text_naming_the_bad_part() {
        let good = known();
        let fields: Vec<&str> = good.split(':').collect();
        let with = |index: usize, value: &str| {
            let mut fields = fields.clone();
            fields[index] = value;
            fields.join(":")
        };
        let cases = [
            ("", "not a password hash"),
            ("pbkdf2-sha256:1000:2:abc", "not a password hash"),
            (&format!("{good}:extra"), "not a password hash"),
            (&with(0, "bcrypt"), "not a known scheme"),
            (&with(0, "PBKDF2-SHA256"), "not a known scheme"),
            (&with(1, "999"), "iteration count"),
            (&with(1, "100001"), "iteration count"),
            (&with(1, "01000"), "iteration count"),
            (&with(1, "+1000"), "iteration count"),
            (&with(1, "99999999999999999999"), "iteration count"),
            (&with(2, "0"), "length"),
            (&with(2, "1025"), "length"),
            (&with(2, "two"), "length"),
            (&with(3, "AAAA"), "salt"),
            (&with(3, "!!!!"), "salt"),
            (&with(4, "AAAA"), "hash"),
            (&with(4, &format!("{}=", fields[4])), "hash"),
        ];
        for (text, part) in cases {
            let error = text.parse::<PasswordHash>().expect_err(text);
            assert!(error.contains(part), "{text}: {error}");
        }
    }

    #[test]
    fn accepts_the_range_limits() {
        let good = known();
        let mut fields: Vec<String> = good.split(':').map(str::to_owned).collect();
        fields[1] = "100000".to_owned();
        assert!(fields.join(":").parse::<PasswordHash>().is_ok());
        fields[2] = "1024".to_owned();
        assert!(fields.join(":").parse::<PasswordHash>().is_ok());
        fields[3] = super::base64::encode(&[1u8; 64]);
        assert!(fields.join(":").parse::<PasswordHash>().is_ok());
    }
}
