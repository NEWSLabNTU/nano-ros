---
id: 721
title: "Gate scripts walk built trees to find tracked files, and the gate that forbids it never reads Python"
status: resolved
type: tech-debt
severity: medium
area: build
related: [issue-0684, issue-0196, issue-0525]
---

# 0721 — exhaustive traversal in the check-* gates

`just check` and every fixture build pay minutes of pure I/O because several
gates discover files by walking the filesystem instead of asking git. The trees
they walk are the built ones.

Measured on this host, 2026-08-20, with the build idle:

| | |
| --- | --- |
| `examples/` on disk | **828 GB** |
| `packages/` on disk | **194 GB** |
| `os.walk("packages")` | 57,646 dirs / 516,507 files visited, to reach **2,371** tracked |
| `Path.rglob("Cargo.toml")` over `examples/` | **did not finish in 300 s** |
| `git ls-files` for both roots | **347 manifests, 0.002 s** |

## The trap: filtering is not pruning

Every instance has the same shape. The script names the directories it does not
want — `SKIP_DIRS = {"generated", "target", "build", ...}` — and then applies
that set to what the walk **has already yielded**. `rglob` still descends every
`target/`, `build-*/` and `third-party/` tree to discover the paths it then
discards.

This repo has learned it twice already and written it down both times:

- `scripts/check-no-tracked-file-find.sh` documents it for `find -prune`: 570x,
  identical output, *"the find burned 0% CPU the whole time — it was never
  compute-bound, it was starved on I/O walking build trees."*
- `scripts/check-image-panic-policy.py` documents it for `glob("**")` (issue
  0684): *"`Path.glob("**")` has to DESCEND a tree to discover it."*

Both fixed their own site. Neither fixed the siblings.

## Why it spread: the gate cannot see Python

`check-no-tracked-file-find.sh` exists precisely to forbid this. Its filter:

```python
if not (f.endswith(".sh") or f.endswith(".just") or f == "justfile"):
    continue
```

It reads only `.sh`, `.just` and `justfile` — **it never opens a `.py` file**,
though the gate is itself written in Python scanning shell. The shell side is
consequently clean: `check-atomic-sync-writes`, `check-no-absolute-model-paths`,
`check-profile-board-mirror`, `check-staleness-probe-exemptions` and
`check-no-direct-kernel-alloc` all carry comments explaining they use `git grep`
rather than `grep -r`. The Python side has 21 walk sites and no coverage at all.

Issue 0196's rule exactly: a gate whose scope is narrower than the rule it
enforces.

## Fixed so far

| script | before | after | commit |
| --- | --- | --- | --- |
| `check-no-std-stdio.py` | >300 s (never finished) | 3 s | `ec2c2d0fa` |
| `check-example-leaf-target-dirs.py` | >90 s (never finished) | 2 s | `d91fff2f8` |

The first moved to `git ls-files` — every file it wants is tracked. The second
legitimately hunts *untracked* dirs, so it still walks, but now prunes `build*`
at descent; the scoping rule below its walk already discarded those, so the
decision simply moved earlier.

## Remaining

Still walking, in rough priority:

- `check-zephyr-kconfig-symbols.py` — 23 s, walks the Zephyr tree. Untracked
  submodule content, so the index does not help; wants scoping instead.
- `check-feature-contract.py` — `os.walk(packages)` for tracked `Cargo.toml`.
  Fast on a warm cache (0.4 s) but visits 516k files to reach 2.3k; converts
  cleanly to `git ls-files`.
- `check-std-census.py` (3 sites), `check-rust-stdio-on-zephyr.py`,
  `check-cpp-ffi-error-mapping.py`, `gen-cli-source-dirs.py`,
  `check-board-facts-delivery.py` — all `rglob` for tracked sources under scoped
  roots. Individually sub-second today because their roots are small, but they
  are the same defect and one added root makes any of them the next
  `check-no-std-stdio`.

Legitimately walking, leave alone: `dep-closure.py` (build dirs),
`prune-superseded-artifacts.py` (deleting untracked output), and the
untracked-artifact half of `check-example-leaf-target-dirs.py`.

## Resolved 2026-08-20 — and the Remaining list above was stale

Re-measured before acting rather than worked from the list, which had been
overtaken by `5a9b77367`:

* **Five of the seven were already converted** — `check-std-census.py`,
  `check-rust-stdio-on-zephyr.py`, `check-cpp-ffi-error-mapping.py`,
  `gen-cli-source-dirs.py`, `check-board-facts-delivery.py` all import
  `lib.tracked` today.
* **`check-zephyr-kconfig-symbols.py` is already scoped** and stays a walk. It
  reads the Zephyr submodule's Kconfig files, which this repo does not track, so
  there is no index to consult; `line_trees()` walks `zephyr/` + `modules/`
  rather than the whole workspace. Measured 0.62 s here. The 23 s in the list
  above predates that scoping — not re-measurable on this host, which has no
  full west workspace.
* **`check-feature-contract.py` is converted, and the reason it had not been is
  the interesting part.** Its `walk-ok:` said the scope is crates ON DISK,
  including the submodule WORKING TREES a plain `git ls-files` does not descend
  into — true, and a contract gate blind to 20 crates is a gate with a hole.
  `--recurse-submodules` descends them. Verified as SETS, not counts: the walk's
  222 manifests are a strict subset of the index's 228, the 7 extras being 6
  under `generated/` (which `is_build_output` already rejects) and one cmake
  template not named `Cargo.toml`. After filtering, old and new agree exactly —
  222 manifests, 1724 Rust files, zero differences either way.

`tracked()` grew a `submodules=True` option rather than the gate calling git
itself, so there is still one spelling of "ask the index".

### Three defects the conversion introduced, and how each was caught

Recorded because each is a way a mechanical conversion goes silently wrong:

1. **Filtering the FILENAME as well as the directories.** `is_build_output`
   matches any name starting with `build`, so `nros-cli-core/src/build_output.rs`
   and two `build.rs` vanished from the gate's view. The walk pruned directories
   at descent and never judged filenames. Caught by comparing as sets — a count
   comparison would have shown 1724 vs 1719 and invited a shrug.
2. **The index consulted per crate.** Calling `tracked()` inside `rust_files`
   is 222 `git ls-files --recurse-submodules` subprocesses: **23.5 s against the
   walk's 0.42 s**, a 56x regression in the name of removing a walk. One call,
   cached and filtered once, then `bisect` per crate: 0.12 s.
3. **`--self-test` looked green while testing nothing.** Its temp trees live
   under `tmp/`, inside the repo but untracked, so a root keyed on `ROOT` found
   no sources there and clauses (c)/(e) reported firing "on a clean tree". The
   root is derived from the path's `packages` component now. The self-test
   caught this; nothing else would have.

Net on this host: `check-feature-contract` 0.42 s -> 0.12 s of discovery
(0.60 s for the whole gate), and it no longer descends build output to find
2.3k files among 516k.

## The actual fix

Widen `check-no-tracked-file-find.sh` to read `.py`, matching `rglob(`,
`glob("**`, and `os.walk(` rooted at a tree that contains build output. Without
that, this list regrows — the two fixed above were both written *after* the
lesson was documented, by authors who had no reason to know.

A warm page cache hides this: `os.walk("packages")` is 0.4 s warm and minutes
cold or under concurrent build I/O, which is exactly when gates run. Measure
cold, or measure with a build running, or the number will lie.
