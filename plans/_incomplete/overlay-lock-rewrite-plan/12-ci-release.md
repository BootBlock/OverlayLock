# 12 CI and release workflows

> **Status: done 2026-11-08, except the first remote run (needs the owner to push).** Pinned:
> `actions/checkout` v7.0.1 `3d3c42e5`, `Swatinem/rust-cache` v2.9.3 `d1527c8f` (annotated tag
> dereferenced to its commit), and `dtolnay/rust-toolchain` at `master` `683b588c` with an explicit
> `toolchain:` input (that action publishes branches, not version tags). The MSRV job runs
> `cargo check --workspace --all-targets --locked`. actionlint 1.7.12: 0 errors (pyflakes not
> installed, so that rule was skipped; no Python in the workflows). Every `ci.yml` step ran clean
> locally in order, the ARM64 release build included.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 done. Locked: OL1, OL20, OL22.

## Touch points

| File | Change |
|---|---|
| `.github/workflows/ci.yml` | New. Push to `main` and every pull request. |
| `.github/workflows/release.yml` | New. Push of a tag matching `v*`. |
| `.github/dependabot.yml` | New. Weekly `cargo` and `github-actions` updates. |

Conflict note: only stage to touch `.github/`.

## Design

- **CI** on `windows-latest`: `actions/checkout`, `dtolnay/rust-toolchain@stable` with `rustfmt`,
  `clippy` and target `aarch64-pc-windows-msvc`, `Swatinem/rust-cache`. Steps: `cargo fmt --all
  --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
  `cargo build --release --target x86_64-pc-windows-msvc`; same for `aarch64-pc-windows-msvc`. A
  second job checks the MSRV with `dtolnay/rust-toolchain@1.98` and `cargo check`.
- Pin each action to a full commit SHA with the version in a comment. Find the current SHAs with
  `gh api repos/<owner>/<action>/git/ref/tags/<tag>`, do not write them from memory.
- `permissions: contents: read` at workflow level in CI.
- **Release**: same toolchain; build both targets with `--locked`; check the tag equals
  `v` + the `Cargo.toml` version and fail if not; zip `overlay-lock.exe`, `LICENSE` and `README.md`
  as `overlay-lock-<version>-<target>.zip`; write `SHA256SUMS.txt`; create the release with
  `gh release create "$tag" --generate-notes` and the files. `permissions: contents: write` only on
  that job.
- The process tests in `tests/` spawn windows. A GitHub Windows runner has an interactive desktop,
  but whether `PostMessageW` tests pass there is unproven until the first push. The `#[ignore]`
  hook tests never run in CI.

## Done when

- [x] Both workflows pass `actionlint` (download the release binary to `%TEMP%` if it is not
      installed) with no findings.
- [x] Every step of `ci.yml` runs clean locally in the same order.
- [x] The first remote run is recorded as outstanding: it needs the owner to create the repository
      and push.
- [x] Status line flipped, stage table updated.
