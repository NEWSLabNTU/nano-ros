---
id: 2645
title: "`nros-toolchain.toml` can only pin a release, so a consumer tracking main vendors nano-ros and sets NROS_SKIP_STALE_CHECK"
status: open
type: enhancement
area: [cli, toolchain]
related: [rfc-0097, rfc-0014, phase-478]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I26.**
GitHub [#4006](https://github.com/NEWSLabNTU/nano-ros/issues/4006).

## What happens

`nros-toolchain.toml` (RFC-0097 D7; the book's "Pinning a version" in
`book/src/guide/build/install-options.md`) takes `version = "<release>"` and
nothing else. A consumer that tracks nano-ros `main` commits, the Autoware
Safety Island (ASI, branch `nano-ros`), has no pin to write, so it:

1. vendors nano-ros as a submodule (`modules/nros`) and builds the CLI from it
   (`build.sh:315-323`, `build_nros_cli()`);
2. exports `NROS_SKIP_STALE_CHECK=1` (`build.sh:321`), because `nros
   configure` writes into that stamped tree and the stale-CLI guard
   (`stale_guard.rs`, `refuse_if_stale`) then fires on a submodule the
   consumer never edited;
3. pins the same commit a second time in `actuation_module/west.yml`, held
   equal to the first by `assert_pin_lockstep` in its CI.

So one fact, "which nano-ros", is written in two places, and the guard that
protects every other build is switched off for this one.

## Two defects, not one

- **A. The pin's vocabulary.** A pin can name a release only.
- **B. The guard was reported to misfire on a consumer's configure.** Not
  reproduced; see B below.

## Design (proposed, beside the release pin) — AWAITING THE USER

A new user-facing schema for `nros-toolchain.toml` (RFC-0097 D7), so it is
the user's decision: nothing below is built until they approve it. Part B is
reported as a plain bug and did not reproduce.

**A.** The pin keeps one key per form, and exactly one of them is set:

```toml
version = "0.6.0-nros11"                          # a release (today)
git     = { rev = "09219edc75c9b9a788590c6ac586a7ba45185eab" }  # a commit on the repo
path    = "../modules/nros"                       # a local checkout
```

- `rev` is a full 40-hex commit (a branch name is refused: it is not a pin).
  The repo defaults to the release repo; `git = { url, rev }` overrides it.
- A `git` pin installs into the store at a path CONSTRUCTED from the pin,
  `toolchains/git-<rev>/` (never an enumerated "newest", issue 1546), by
  fetching that one commit and running the tree's own `scripts/bootstrap.sh`.
  It is built once per rev and never rebuilt in place; a moved pin is a new
  directory.
- A `path` pin runs that checkout's own `bootstrap.sh`-built CLI. The stale
  guard applies to it as to any checkout; after B it fires only on real edits.
- RFC-0090's codegen version check and every `nros doctor` line work the same
  for all three: the launcher reports which form it read (`info: nano-ros
  git 09219edc75 (nros-toolchain.toml)`).
- `nros pin --rev <sha>` / `--path <dir>` move the pin like `nros pin
  <version>` does (RFC-0097 D11: CI refuses rather than writes one).
- West's own pin stays west's. A Zephyr consumer names the same commit there,
  and `nros doctor` reports a pin and a west revision that disagree, so a
  consumer's lockstep script can go.

**B. Not reproduced on pin `ea94566e87` (2026-11-16).** The island ran its
FreeRTOS-posix CI lane with `NROS_SKIP_STALE_CHECK` unset, after its own
`build_nros_cli()`: `nros configure`, `sync`, `build`, `configure --check`,
`check --bringup/--workspace` and 13 `dev image leaf-system` runs all passed
with no stale refusal. The source stamp already leaves out git-ignored files
(`ls-files --others --exclude-standard`). The guard was ON there only because
the island's `build.sh` read the submodule as dirty, from three nested
gitlinks out of sync with their recorded pins (`micro-cdr`, `nuttx`,
`nuttx-apps`); on a clean submodule `build.sh` sets the skip itself. So B is
open only as a question for the island's script, not as a nano-ros defect:
with A, the island drops both its CLI build and the skip.

## Acceptance

- ASI pins one commit in `nros-toolchain.toml` and deletes `build_nros_cli()`
  and `NROS_SKIP_STALE_CHECK=1`.
- A `path` pin over a clean checkout runs `nros configure` and then any
  guarded command without a stale refusal (measured at `ea94566e87` with the
  island's own CLI build); an edit to a tracked CLI source still refuses.
- A `git` pin with a branch name or a short sha is refused with the reason.
