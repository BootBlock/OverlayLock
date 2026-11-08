# 01 Spike: what a UAC prompt gives

> **Status: not started.**

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: none. Locked: UY1, UY3.

## Steps

1. Extend the stage 10 logger (rebuild it in `%TEMP%`: out-of-context
   `EVENT_SYSTEM_DESKTOPSWITCH` hook, session notifications, `OpenInputDesktop` name) to also log,
   on each switch: the foreground window's process before and after, `consent.exe` present or
   not (process snapshot), and any `EVENT_SYSTEM_FOREGROUND` events in between.
2. With the owner at the desktop, record: Ctrl+Alt+Del then Cancel; Ctrl+Alt+Del then Task
   Manager; a UAC prompt (for example `Start-Process cmd -Verb RunAs`) answered Yes; one answered
   No. Ask with the question tool, and say plainly that no lock appears during the spike.
3. Look for a signal present for UAC and absent for Ctrl+Alt+Del (the strongest candidate is
   `consent.exe` running during the visit). Check it does not need polling while idle (UY2): a
   snapshot taken only when a visit ends is acceptable.
4. Write the sequences and the chosen signal (or "none") in this file, then write stage 02.

## Done when

- [ ] The four sequences are recorded here.
- [ ] A signal is chosen, or the plan is closed as not possible under UY1.
