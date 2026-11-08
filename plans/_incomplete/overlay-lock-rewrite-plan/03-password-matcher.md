# 03 Password matcher

> **Status: done 2026-11-08.** Clippy is clean apart from `dead_code`, which clears when 05 uses the type.
> The ring-order mutation failed 4 tests (overlap among them). Added a ring-wrap test.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 done. Locked: OL6.

## Touch points

| File | Change |
|---|---|
| `src/password.rs` | New. `PasswordMatcher`. |
| `src/main.rs` | `mod password;` only. |

Conflict note: 04 also adds `mod` lines to `src/main.rs` in this block.

## Design

```rust
pub struct PasswordMatcher { password: Box<[u16]>, ring: Box<[u16]>, next: usize, filled: usize }
impl PasswordMatcher {
    pub fn new(password: Vec<u16>) -> Self;          // panics in debug if empty: cli forbids it
    pub fn push(&mut self, unit: u16) -> bool;       // true when the last N units equal the password
    pub fn clear(&mut self);
}
```

- `push` writes into the ring, advances `next` modulo N, and compares the ring read from `next`
  (oldest) against `password` without allocating. It returns `false` until N units arrived.
- The type is `Send` (it moves to the hook thread in 08) and has no interior mutability.
- No constant-time compare: OL6 and the README say this is not a security product.

## Done when

- [x] Tests: exact match; match after noise (`"xxgo"`); no match on a prefix; overlap
      (`"aab"` against password `"ab"`); a one-unit password; case sensitivity (`"GO"` fails for
      `"go"`); a surrogate pair password (`"🔒1"`) matched unit by unit; `clear` forgets history;
      Backspace (U+0008) counts as a unit, as in Lite.
- [x] Prove one test can fail: break the ring index on purpose, see the overlap test fail, revert.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.
