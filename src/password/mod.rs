//! Matches the last typed UTF-16 code units against the unlock password or its hash.

pub mod base64;
pub mod hash;
pub mod pbkdf2;
pub mod worker;

use hash::PasswordHash;
use pbkdf2::Pbkdf2;

/// What unlocks the lock: the password itself (`--password`) or its hash (`--password-hash`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Password {
    /// The password as UTF-16 code units, never empty.
    Plain(Vec<u16>),
    /// The password's hash.
    Hash(PasswordHash),
}

/// How a full ring is checked.
#[derive(Debug)]
enum Check {
    /// Compared with the password itself.
    Plain(Box<[u16]>),
    /// Hashed and compared with the password hash. `bytes` is the ring as little-endian bytes,
    /// oldest first, allocated once.
    Hash {
        hash: PasswordHash,
        pbkdf2: Pbkdf2,
        bytes: Box<[u8]>,
    },
}

/// Remembers the last N typed UTF-16 code units, where N is the password length, and reports when
/// they are the password (OL6).
///
/// The ring is allocated once, so [`PasswordMatcher::push`] never allocates. The comparison is
/// case-sensitive and every unit counts, control characters included. With a password hash, each
/// push after the first N runs one PBKDF2 derivation, which takes a millisecond or two, so a hashed
/// matcher runs on the match thread ([`worker`]), never on a hook or window procedure.
#[derive(Debug)]
pub struct PasswordMatcher {
    check: Check,
    ring: Box<[u16]>,
    next: usize,
    filled: usize,
}

impl PasswordMatcher {
    /// Initialises a new instance of the [`PasswordMatcher`] type for the given password. Fails
    /// only if a hashed password's PBKDF2 provider cannot be opened.
    ///
    /// The command line forbids an empty password, so an empty one is a programming error.
    pub fn new(password: Password) -> windows::core::Result<Self> {
        let (check, len) = match password {
            Password::Plain(units) => {
                let len = units.len();
                (Check::Plain(units.into_boxed_slice()), len)
            }
            Password::Hash(hash) => {
                let len = hash.length();
                let check = Check::Hash {
                    hash,
                    pbkdf2: Pbkdf2::open()?,
                    bytes: vec![0; len * 2].into_boxed_slice(),
                };
                (check, len)
            }
        };
        debug_assert!(len > 0, "the password must not be empty");
        Ok(Self {
            check,
            ring: vec![0; len].into_boxed_slice(),
            next: 0,
            filled: 0,
        })
    }

    /// Adds one typed code unit.
    ///
    /// Returns `true` when the last N units, oldest first, are the password; otherwise, `false`.
    pub fn push(&mut self, unit: u16) -> bool {
        let len = self.ring.len();
        if len == 0 {
            return false;
        }
        self.ring[self.next] = unit;
        self.next = (self.next + 1) % len;
        if self.filled < len {
            self.filled += 1;
        }
        if self.filled < len {
            return false;
        }
        // The oldest unit is at `next`: take the two halves of the ring in order.
        let (newer, older) = self.ring.split_at(self.next);
        match &mut self.check {
            Check::Plain(password) => {
                let (head, tail) = password.split_at(older.len());
                older == head && newer == tail
            }
            Check::Hash {
                hash,
                pbkdf2,
                bytes,
            } => {
                for (slot, unit) in bytes
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(older.iter().chain(newer))
                {
                    *slot = unit.to_le_bytes();
                }
                hash.matches(pbkdf2, bytes)
            }
        }
    }

    /// Forgets every unit typed so far.
    pub fn clear(&mut self) {
        self.ring.fill(0);
        if let Check::Hash { bytes, .. } = &mut self.check {
            bytes.fill(0);
        }
        self.next = 0;
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{Password, PasswordMatcher, hash::PasswordHash};

    /// A matcher of each kind for `password`: the plain one, then the hashed one. Every test runs
    /// on both, so the hashed matcher keeps the plain one's behaviour.
    fn matchers(password: &str) -> [PasswordMatcher; 2] {
        let units: Vec<u16> = password.encode_utf16().collect();
        let hash = PasswordHash::make(&units).expect("the hash is made");
        [Password::Plain(units), Password::Hash(hash)]
            .map(|password| PasswordMatcher::new(password).expect("the matcher opens"))
    }

    /// Pushes every unit of `typed` and returns the result of each push.
    fn type_text(matcher: &mut PasswordMatcher, typed: &str) -> Vec<bool> {
        typed
            .encode_utf16()
            .map(|unit| matcher.push(unit))
            .collect()
    }

    /// Checks that typing `typed` against `password` gives `expected` with both kinds of matcher.
    fn check(password: &str, typed: &str, expected: &[bool]) {
        for (kind, mut matcher) in ["plain", "hash"].into_iter().zip(matchers(password)) {
            assert_eq!(
                type_text(&mut matcher, typed),
                expected,
                "{kind}: {typed:?}"
            );
        }
    }

    #[test]
    fn exact_match() {
        check("go", "go", &[false, true]);
    }

    #[test]
    fn match_after_noise() {
        check("go", "xxgo", &[false, false, false, true]);
    }

    #[test]
    fn prefix_does_not_match() {
        check("gopher", "gop", &[false; 3]);
    }

    #[test]
    fn overlap_matches() {
        check("ab", "aab", &[false, false, true]);
    }

    #[test]
    fn long_password_wraps_ring() {
        check("abc", "xyzabc", &[false, false, false, false, false, true]);
        check("abc", "xabcab", &[false, false, false, true, false, false]);
    }

    #[test]
    fn one_unit_password() {
        check("q", "xq", &[false, true]);
    }

    #[test]
    fn match_is_case_sensitive() {
        check("go", "GO", &[false, false]);
        check("go", "gO", &[false, false]);
    }

    #[test]
    fn surrogate_pair_matches_unit_by_unit() {
        assert_eq!("\u{1F512}1".encode_utf16().count(), 3);
        check("\u{1F512}1", "x\u{1F512}1", &[false, false, false, true]);
    }

    #[test]
    fn clear_forgets_history() {
        for mut matcher in matchers("go") {
            assert!(!matcher.push(u16::from(b'g')));
            matcher.clear();
            assert!(!matcher.push(u16::from(b'o')));
            assert_eq!(type_text(&mut matcher, "go"), [false, true]);
        }
    }

    #[test]
    fn backspace_counts_as_a_unit() {
        // Backspace is a character, not a delete: "gx\bo" ends with "\bo", not "go".
        check("go", "gx\u{8}o", &[false; 4]);
        check("g\u{8}", "g\u{8}", &[false, true]);
    }

    /// The hashed matcher compares the ring oldest first: a rotation of the password, which a
    /// ring read from the wrong end would give, never matches.
    #[test]
    fn hashed_matcher_reads_the_ring_oldest_first() {
        let [_, mut hashed] = matchers("abc");
        assert_eq!(type_text(&mut hashed, "bca"), [false; 3]);
        assert_eq!(type_text(&mut hashed, "cab"), [false; 3]);
        assert_eq!(type_text(&mut hashed, "c"), [true]);
    }
}
