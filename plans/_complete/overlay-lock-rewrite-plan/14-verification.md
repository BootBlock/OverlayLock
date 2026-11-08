# 14 Verification

> **Status: done 2026-11-08.** The owner waived Lite parity (step 6) on 2026-11-08, on the live
> behaviour matrix in step 5, and closed the stage 07 scale-change check as not possible on this
> machine. The first remote CI run is green (stage 12).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 to 13 done. Locked: all.

## Steps

1. **Fresh build.** `cargo clean`, then `cargo build --release` for both targets, `cargo fmt --all
   --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, then
   `cargo test --workspace -- --ignored` (the hook tests; they use `--timeout 20s`).
2. **Size and dependencies.** Record the release exe size for both targets. `dumpbin /dependents`
   lists only system DLLs (no `VCRUNTIME140.dll`, no `api-ms-win-crt-*`). `cargo tree -p
   overlay-lock -e normal` shows only `windows` (and its own crates) and `lexopt`.
3. **Start time.** Measure 20 runs of process start to first overlay shown, without and with
   `--message`: a small test harness spawns the exe and polls `EnumWindows` for the visible overlay
   at 1 ms steps. Record median and worst. Without `--message`, record how long `d2d1.dll` and
   `dwrite.dll` take to load, from Process Monitor or ETW image-load events. If the sum is under
   2 ms, record it and change nothing. If it is more, report it as a finding with delay-loading as
   the fix.
4. **Idle cost.** While locked for 60 s, CPU time of the process stays at 0 ms after start-up
   (`Get-Process overlay-lock | Select CPU` at 5 s and 60 s), and the working set is recorded.
5. **Behaviour matrix**, live, each with `--timeout 60s` as the safety net, each on every monitor:
   default; `--monitors cursor`; `--colour`; `--message` with every text option; `--block-keys`;
   `--block-mouse`; both; `--ignore-alt-f4` in both modes; `--yield-on-ctrl-alt-del`; a second
   instance; a resolution change while locked; a monitor unplugged while locked; a launch from
   a Stream Deck (or `Start-Process` from a background script) while another window has focus.
   Record pass or fail for each in this file.
6. **Lite parity.** Run `P:\Source\DotNet\OverlayLockLite\OverlayLockLite\bin\Release\net9.0-windows7.0\OverlayLockLite.exe`
   (password `go`) and `overlay-lock -p go --monitors cursor` one after the other: the same monitor
   goes black, the cursor hides, and `go` unlocks both.
7. **Review.** Run the `auto-review` skill at `medium` over the whole tree. The repository has no
   commit yet, so if the skill needs a base commit, record the review as waiting on the owner's
   first commit and say so in the finish. Fix every finding it reports.
8. **Pictures.** Capture the message screen and the help output for `gambit_record_change`.

## Record

1. **Fresh build.** `cargo clean` (2.4 GiB), then both release builds, fmt, clippy
   (`-D warnings`), `cargo test --workspace` (71 unit + 14 process tests) and the 7 `--ignored`
   hook tests: all pass. They were rerun after the review fixes: all pass.
2. **Size and dependencies.** Final release builds: x64 `overlay-lock.exe` 386,560 bytes, ARM64
   375,296 bytes. `dumpbin /dependents`: `user32`,
   `wtsapi32`, `kernel32`, `d2d1`, `dwrite`, `gdi32`, `oleaut32`, `ntdll`, `bcryptprimitives` and
   the `api-ms-win-core-synch-l1-2-0` and `api-ms-win-core-winrt-error-l1-1-0` API sets: no
   `VCRUNTIME140.dll`, no `api-ms-win-crt-*`. `cargo tree -p overlay-lock -e normal`: `lexopt`,
   `windows` and its own crates, and `windows-numerics` (a `windows` crate, used directly for
   `Vector2`).
3. **Start time** (throwaway harness in `%TEMP%`, 20 runs each, spawn to visible overlay, 1 ms
   polling): without `--message` median 24.5 ms, worst 484.9 ms (the first, cold run), best 18.8
   ms; with `--message` median 26.7 ms, worst 29.9 ms. `LoadLibrary` of `d2d1.dll` + `dwrite.dll`
   in a fresh process: median 1.8 ms, worst 2.4 ms. The median is under 2 ms, so nothing changes;
   delay-loading would save at most about 2 ms.
4. **Idle cost.** Locked for 60 s: CPU time 31.25 ms at 5 s and 31.25 ms at 58 s (0 ms after
   start-up); working set 14.8 MB, private 2.2 MB.
5. **Behaviour matrix** (live, owner at the desktop, `--timeout 60s`, both monitors unless named):

   | Case | Result |
   |---|---|
   | Default | Pass (check 1) |
   | `--monitors cursor` | Pass (checks 5, 6) |
   | `--colour` | Pass (check 4, README message run) |
   | `--message` with every text option | Pass (check 4: colour, size, font; NoSuchFont falls back) |
   | `--block-keys` | Pass (check 7) |
   | `--block-mouse` | Pass on rerun; first run's failure not reproduced (stage 09) |
   | Both hooks | Pass (checks 10, 11, README cat example) |
   | `--ignore-alt-f4`, focus mode | Pass (README run 3; process test) |
   | `--ignore-alt-f4`, hook mode | Pass (check 7; hook test) |
   | `--yield-on-ctrl-alt-del` | Pass (checks 10, 11; Win+L does not yield) |
   | Second instance | Pass (process test, exit 3) |
   | Resolution change while locked | Pass after the raise fix (check 6) |
   | Monitor unplugged while locked | Pass after the 10 s raise and the focus handover |
   | Stream Deck launch with another window focused | Pass (Open action; Firefox had the focus) |
   | Stream Deck text action with `--block-keys` | Pass after the `VK_PACKET` fix |
6. **Lite parity.** Waived by the owner on 2026-11-08; the step 5 behaviour matrix stands in.
7. **Review.** `auto-review` at `medium` (lanes 1 to 4). The repository has no commit, so the
   whole tree was reviewed as new. 7 findings, all fixed: the stage 14 refocus broke OL10 (now a
   handover before the destroy); the README overstated the yield; a failed reconcile stopped
   halfway (now applies every change and returns the first error); `UnlockCause` was never read
   (removed); a colour-table length assertion could not fail; the stage 05 status line was stale;
   plan-stage wording was in shipped code.
8. **Pictures.** `target/shots/message.png` and `target/shots/help-console.png`.

## Done when

- [x] Steps 1 to 8 done, each result recorded in this file.
- [x] Every failure is fixed in its stage's files and the step rerun, or reported as outstanding.
- [x] Status line flipped, stage table updated, overview status line set to the true state.
