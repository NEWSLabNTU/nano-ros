---
id: 2645
title: "`nros-toolchain.toml` can only pin a release, so a consumer tracking main vendors nano-ros and sets NROS_SKIP_STALE_CHECK"
status: open
type: enhancement
severity: medium
area: [cli, toolchain]
related: [rfc-0097, rfc-0014, phase-478]
found: 2026-11-16
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I26.**
GitHub [#4006](https://github.com/NEWSLabNTU/nano-ros/issues/4006).

**Priority: normal (user ruling, 2026-11-16, superseding the "low" set
earlier that day).** Fixes ship in batches, and until nros12 the island runs
a from-source CLI at a `main` commit (its dev mode), so part A is on the
island's path again. Severity is back to `medium`. It still waits on the
user's design review: nothing in part A is built until they approve it.

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
version = "0.6.0-nros11"                                     # a release (today)
git     = "https://github.com/NEWSLabNTU/nano-ros"           # a commit on a repo,
rev     = "09219edc75c9b9a788590c6ac586a7ba45185eab"         #   cargo's spelling
path    = "../modules/nros"                                  # a local checkout
```

Cargo's spelling, so a reader who knows a cargo git dependency reads it
without a manual: `git` is the URL and `rev` sits beside it, as in
`{ git = "<url>", rev = "<sha>" }`. `rev` is valid only with `git`.

- `rev` is a full 40-hex commit (a branch name, a tag or a short sha is
  refused, naming the full-sha rule: none of them is a pin). There is no
  default repo: `git` is always written, as in cargo.
- `path` is relative to the directory that holds `nros-toolchain.toml`, as a
  cargo `path` dependency is relative to its `Cargo.toml`; an absolute path is
  taken as written. It names a nano-ros checkout (a directory with the
  checkout marker), and the pin is refused otherwise.

**What a `git` pin needs and builds.** It installs into the store at a path
CONSTRUCTED from the pin, `toolchains/git-<rev>/` (never an enumerated
"newest", issue 1546): it fetches that one commit and runs the tree's own
`scripts/bootstrap.sh` front door (the no-subcommand form). That builds the
`nros` CLI, the `nros-launch-resolve` resolver and the companion packages in
`scripts/lib/cli-companion-packages.txt`, so the host needs `git`, `cargo`
(a rustup toolchain; the tree's `rust-toolchain.toml` picks the version), a C
compiler and linker, Python 3 with its headers (the resolver embeds CPython
through pyo3), and network access for the crates. It needs no ROS and no
ament: the resolver needs CPython, not ROS. bootstrap.sh installs rustup when
`cargo` is absent; the pin install does NOT, and refuses instead, naming
rustup, because a toolchain install that edits `~/.cargo` and the shell rc is
the user's decision, not a side effect of reading a pin.

**The install is atomic.** It builds in a sibling
`toolchains/.git-<rev>.partial-<pid>/` and renames it to `git-<rev>/` only
after the build and `nros source-stamp` both succeed; on any failure the
partial directory is deleted. A crash that skips the cleanup leaves only a
`.partial-*` name, which no lookup reads (the path is constructed, so
`git-<rev>/` either exists complete or not at all) and the next install of
any pin deletes when its pid is not live. Two installs of one rev race to the
rename; the loser deletes its own copy and uses the winner's. A `git-<rev>/`
is never rebuilt in place, and a moved pin is a new directory. Renaming a
built tree is safe because the CLI is already relocatable: a release binary
is built on a CI runner and runs wherever it is installed, and the checkout a
binary belongs to is found from its own path at run time
(`stale_guard.rs`, `checkout_root_of`), not recorded at build time. The
`env!("CARGO_MANIFEST_DIR")` reads in `nros-cli-core` are in its tests. Only
cargo's fingerprints keep the partial path, and nothing rebuilds a
`git-<rev>/` in place.

**A `path` pin** runs that checkout's own `bootstrap.sh`-built CLI. The
stale guard applies to it as to any checkout, and after B it fires only on
real edits.

**What a pin costs per invocation.** Every `nros` invocation, the launcher
(`nros-launcher/src/dispatch.rs`) walks up from the cwd to the nearest
`nros-toolchain.toml`, reads it and `exec`s the pinned binary with argv
unchanged; that is the same for all three forms. A `path` pin's binary then
runs the stale guard (`stale_guard.rs`, `refuse_if_stale`), and only for a
guarded verb (`sync`, `configure`, `plan`, `ws`, `codegen`,
`codegen-system`, `generate-rust`, `setup`): it recomputes the checkout's
source stamp (`git ls-files` over the CLI's source dirs, hashed). A `git`
pin's store binary has no checkout to stamp, so it pays only the walk and
the exec. Shell completion is not a guarded verb, so a completion request
pays the walk and one `exec` on every form (#4082's 100 ms budget), and never
the stamp.

Measured 2026-11-17 on a release build of the CLI (host load
about 23, median of 15): one `nros --version` from a directory with no pin,
which is the launcher's walk plus a process start, 1.9 ms (max 3.0); the
stamp the guard computes, `nros source-stamp` in a checkout, 18.3 ms (max
21.0). So a pinned completion request pays about two process starts, under
5 ms of #4082's 100 ms budget, and a guarded verb on a `path` pin adds about
18 ms once per invocation.
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
