//! Matches the last typed UTF-16 code units against the unlock password.

/// Remembers the last N typed UTF-16 code units, where N is the password length, and reports when
/// they equal the password.
///
/// The ring is allocated once, so [`PasswordMatcher::push`] never allocates. The comparison is
/// case-sensitive and every unit counts, control characters included, as in OverlayLockLite.
#[derive(Debug)]
pub struct PasswordMatcher {
    password: Box<[u16]>,
    ring: Box<[u16]>,
    next: usize,
    filled: usize,
}

impl PasswordMatcher {
    /// Initialises a new instance of the [`PasswordMatcher`] type for the given password.
    ///
    /// The command line forbids an empty password, so an empty one is a programming error.
    pub fn new(password: Vec<u16>) -> Self {
        debug_assert!(!password.is_empty(), "the password must not be empty");
        let ring = vec![0; password.len()].into_boxed_slice();
        Self {
            password: password.into_boxed_slice(),
            ring,
            next: 0,
            filled: 0,
        }
    }

    /// Adds one typed code unit.
    ///
    /// Returns `true` when the last N units, oldest first, equal the password; otherwise, `false`.
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
        // The oldest unit is at `next`: compare the two halves of the ring in order.
        let (newer, older) = self.ring.split_at(self.next);
        let (head, tail) = self.password.split_at(older.len());
        older == head && newer == tail
    }

    /// Forgets every unit typed so far.
    pub fn clear(&mut self) {
        self.ring.fill(0);
        self.next = 0;
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::PasswordMatcher;

    fn matcher(password: &str) -> PasswordMatcher {
        PasswordMatcher::new(password.encode_utf16().collect())
    }

    /// Pushes every unit of `typed` and returns the result of each push.
    fn type_text(matcher: &mut PasswordMatcher, typed: &str) -> Vec<bool> {
        typed
            .encode_utf16()
            .map(|unit| matcher.push(unit))
            .collect()
    }

    #[test]
    fn exact_match() {
        assert_eq!(type_text(&mut matcher("go"), "go"), [false, true]);
    }

    #[test]
    fn match_after_noise() {
        assert_eq!(
            type_text(&mut matcher("go"), "xxgo"),
            [false, false, false, true]
        );
    }

    #[test]
    fn prefix_does_not_match() {
        assert_eq!(type_text(&mut matcher("gopher"), "gop"), [false; 3]);
    }

    #[test]
    fn overlap_matches() {
        assert_eq!(type_text(&mut matcher("ab"), "aab"), [false, false, true]);
    }

    #[test]
    fn long_password_wraps_ring() {
        assert_eq!(
            type_text(&mut matcher("abc"), "xyzabc"),
            [false, false, false, false, false, true]
        );
        assert_eq!(
            type_text(&mut matcher("abc"), "xabcab"),
            [false, false, false, true, false, false]
        );
    }

    #[test]
    fn one_unit_password() {
        assert_eq!(type_text(&mut matcher("q"), "xq"), [false, true]);
    }

    #[test]
    fn match_is_case_sensitive() {
        assert_eq!(type_text(&mut matcher("go"), "GO"), [false, false]);
        assert_eq!(type_text(&mut matcher("go"), "gO"), [false, false]);
    }

    #[test]
    fn surrogate_pair_matches_unit_by_unit() {
        let mut matcher = matcher("\u{1F512}1");
        assert_eq!("\u{1F512}1".encode_utf16().count(), 3);
        assert_eq!(
            type_text(&mut matcher, "x\u{1F512}1"),
            [false, false, false, true]
        );
    }

    #[test]
    fn clear_forgets_history() {
        let mut matcher = matcher("go");
        assert!(!matcher.push(u16::from(b'g')));
        matcher.clear();
        assert!(!matcher.push(u16::from(b'o')));
        assert_eq!(type_text(&mut matcher, "go"), [false, true]);
    }

    #[test]
    fn backspace_counts_as_a_unit() {
        // Backspace is a character, not a delete: "gx\bo" ends with "\bo", not "go".
        assert_eq!(type_text(&mut matcher("go"), "gx\u{8}o"), [false; 4]);
        assert_eq!(type_text(&mut matcher("g\u{8}"), "g\u{8}"), [false, true]);
    }
}
