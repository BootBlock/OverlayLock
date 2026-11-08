# OverlayLock Rust rewrite plan - AI seed

Paste the block below into a fresh agent session to execute this plan end to end.

---

You are executing the OverlayLock Rust rewrite plan. The repository is `P:\Source\Rust\OverlayLock` (not yet a git repository: stage 01 runs `git init -b main`; the public remote `github.com/BootBlock/OverlayLock` does not exist yet). The plan folder is `P:\Source\Rust\OverlayLock\plans\overlay-lock-rewrite-plan\`. The app being ported is `P:\Source\DotNet\OverlayLockLite` (read only: never edit it). Once stage 01 has created `CLAUDE.md`, read it and obey it.

Read `00-overview.md` in full before anything else. Decisions OL1-OL22 are LOCKED: do not re-litigate them. If one proves impossible, stop and say so rather than redesigning around it.

Execute every stage, 01 to 14, to the end; 14 is the end-to-end verification. Run the blocks strictly in this order, and inside a block take the stages in any order: 01 → 03+04+11+12 → 02 → 05 → 06 → 07 → 08 → 09 → 10 → 13 → 14. Never overlap any two of 05-10 (one lane through `src/overlay/`, `src/hooks/`, `src/main.rs`). If you fan block 2 out to subagents, give each lane its own git worktree and a name, pass `agent:'<lane>'` on every Gambit coordination call, and call `gambit_finish_lane` when the lane ends.

Per stage: read the stage file; do exactly what it says; run its Done-when checks; tick its boxes and flip its status line to `done <date>` with one line on anything that differs from the plan; update the stage table in `00-overview.md`; then move straight on to the next stage. Never commit, tag or push. At the end, say what is uncommitted and let me run `/commit`.

Watch-outs:
- All: build with `cargo build` from the repository root; the release profile is fat LTO, so use debug builds while iterating and release builds only where a check names one. A running `overlay-lock.exe` locks the exe and fails the link: unlock it (or let its `--timeout` run out) before rebuilding.
- All: every live run you start yourself carries `--timeout` (60 s or less) so a fault can never leave the desktop locked. Never start a lock without it.
- All: live checks need the owner at the desktop for key presses and eyes. Ask with the question tool when one is due. If the owner does not answer, record the check as outstanding and continue.
- All: `windows` 0.62 API names and feature flags change between releases. Check each function's path and feature in the crate docs before you use it; never guess a signature.
- 01: check each crate version with `cargo search` before pinning; confirm crt-static with `dumpbin /dependents`.
- 02: copy the CSS named colours from the CSS Color 4 specification, not from memory.
- 04 before 02: `Monitors` lives in `src/monitors.rs`; `cli.rs` imports it.
- 05: tests find the overlay by class `OverlayLock.Overlay` and the child PID. Tests always redirect output, so a message box can never block them.
- 08: the hook proc must never allocate or block: Windows removes a slow hook silently.
- 10: the spike comes first and needs the owner to press Ctrl+Alt+Del and Win+L. Throwaway spike code goes in `%TEMP%`, never in the repository.
- 11: match SpectraWrite's icon style exactly (read `P:\Source\DotNet\SpectraWrite\scripts\generate-spectrawrite-icon.ps1` first); show the glyph candidates with `gambit_pick_variant`; if the owner is away, file the pick with `gambit_file_decision`, carry on, and finish 11 before 14.
- 12: find action commit SHAs with `gh api`; the first remote CI run is outstanding until I push.
- 14: run the `auto-review` skill at `medium` only.

Concurrency: other agent sessions may be working on this machine right now. On a build or type error in code you did not touch, or odd Gambit or MCP behaviour, assume another agent is mid-edit or mid-build: wait 20-90 s and retry, re-read the file, and never "fix" someone else's half-written code. Never kill a process you did not start.

Run the plan as far as you can without my intervention. Do not stop to check in between stages, do not ask permission to continue, and do not ask me to confirm work that the plan already decided. If something blocks you, first do every remaining piece of work that does not depend on the answer, then come back to me. Only stop for: a locked decision that proves impossible; a secret, credential, or account you don't have; a production deploy, destructive action, or anything else outward-facing; or a genuine judgement call that is mine to make. Everything else - build failures, design detail, ambiguity a careful colleague would resolve - you resolve yourself and note it.

When the plan is done, or as done as it can get: report a short table of the stages and what actually happened, state plainly anything not finished and why (every live check left outstanding, the first CI run, the review if it waited on a commit), file the plan folder per `plans/_complete/README.md` (`_complete` only when every follow-up is closed; otherwise `_incomplete` with the blocker in the status line), record the user-visible changes with `gambit_record_change`, and then ask me, in one go, every question that accumulated along the way.
