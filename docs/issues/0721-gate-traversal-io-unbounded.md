---
id: 721
title: "Gate scripts walk built trees to find tracked files, and the gate that forbids it never reads Python"
status: open
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

## The actual fix

Widen `check-no-tracked-file-find.sh` to read `.py`, matching `rglob(`,
`glob("**`, and `os.walk(` rooted at a tree that contains build output. Without
that, this list regrows — the two fixed above were both written *after* the
lesson was documented, by authors who had no reason to know.

A warm page cache hides this: `os.walk("packages")` is 0.4 s warm and minutes
cold or under concurrent build I/O, which is exactly when gates run. Measure
cold, or measure with a build running, or the number will lie.
