# Security policy

## Supported versions

Only the latest release gets security fixes.

## Reporting a vulnerability

Report a vulnerability privately through
[GitHub's private vulnerability reporting](https://github.com/BootBlock/OverlayLock/security/advisories/new),
not in a public issue. Include the OverlayLock version, the Windows version, the command line you
used and the steps that show the problem.

## What counts

OverlayLock is a privacy screen, not a security boundary: the README's
"What it is, and what it is not" section lists the ways out that are there by design, such as
Ctrl+Alt+Del, Alt+F4 and the timeout. A report is most useful when it shows the lock doing
something the README says it does not, for example letting input through while `--block-keys` or
`--block-mouse` is on, or ending without the password, the timeout or one of the documented ways
out.
