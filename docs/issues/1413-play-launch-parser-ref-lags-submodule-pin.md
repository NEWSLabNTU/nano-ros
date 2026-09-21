---
id: 1413
title: "`[tool.play_launch_parser].source.ref` lags the `play_launch` submodule
  pin by ~40 commits, three documents assert the two cannot disagree, and no
  gate measures it — and the obvious bump ships a SILENT mis-parse"
status: open
type: tech-debt
area: cli, build
found: 2026-09-21
related: [issue-1273, issue-0897, issue-0609, issue-0507, issue-0500, rfc-0060, rfc-0099]
---

## The drift

Two values name the same component and they do not match:

| where | value |
| --- | --- |
| `nros-sdk-index.toml` `[tool.play_launch_parser].upstream` / `.source.ref` | `838ce948` |
| `packages/cli/third-party/play_launch` gitlink (`git ls-tree HEAD`) | `07f0461e` |

`838ce948` is a STRICT ANCESTOR of `07f0461e` (`git merge-base --is-ancestor`
succeeds), ~40 commits behind. The window contains, among others,
`fix(pyexec): carry global parameters into the Python half, ABI 3 to 4 (#0028)`,
`chore: pin ros-launch-manifest to v0.1.34`, and the whole of issue 0897's
libpython split.

So the BINARY half — what `nros setup --tool play_launch_parser` installs — is
built from an older commit than the LIBRARY half `nros-launch-resolve`
path-deps out of the submodule. That is the shape of issues 0609 (zenoh router)
and 0507 (cyclonedds): one component, two pins, nothing keeping them in step.

## Three documents assert the opposite, and nothing checks any of them

1. `nros-sdk-index.toml`, beside the entry: *"its `ref` is the submodule pin so
   the two cannot disagree."*
2. `nano-ros-sdk`'s `scripts/build-play_launch_parser.sh`: *"Keep this in
   lockstep with nano-ros's `packages/cli/third-party/play_launch` submodule
   pin: the two must name the SAME commit, or the binary this dist ships and
   the library `nros-launch-resolve` links diverge."*
3. `just/workspace.just`'s header: *"Kept in lockstep with
   `nros-sdk-index.toml::[tool.play_launch_parser]`"* and *"The version is the
   SUBMODULE COMMIT, so the stamp invalidates exactly when the pin moves."*
   (That last sentence describes a world phase-422 W1 retired: the recipe is a
   one-line forwarder to `nros setup --tool`, so the version is the index's
   `0.1.0-nrosN`, not the submodule commit.)

Three claims, zero measurements. Prose asserting an invariant nothing checks is
how this drifted and stayed drifted — the same reason `check-dist-or-reason`
and friends exist one table over.

## What makes it worse than a stale pin: the obvious fix is WRONG

The natural repair is "move `source.ref` to the submodule pin and re-cut the
dist." **Measured, that ships a regression**, because issue 0897 W2b/W3 moved
pyo3 OUT of the `play_launch_parser` crate inside the drift window: the Python
half is now `pyexec` (a cdylib) loaded at runtime by `pyload`, and `pyload` is
depended on by `resolve/`, NOT by the standalone CLI's `main.rs`.

Built both ways on this host and run against the same two launch files:

| | `838ce948` (the published `0.1.0-nros1` asset) | `07f0461e` (the submodule pin) |
| --- | --- | --- |
| `DT_NEEDED` | `libpython3.10.so.1.0`, libgcc_s, libc, ld | **libgcc_s, libc, ld — no libpython at all** |
| `.launch.py` | resolves (`/from_python`) | **hard error**: *"no Python backend is loaded"* |
| `$(eval '1 + 1')` in XML | resolves (`/from_eval_2`) | **silently unevaluated**: `/from_eval_$(eval '1 + 1')` |

The `.launch.py` arm is a clean, catchable error — 0897 designed it that way on
purpose. The `$(eval …)` arm is the dangerous one: exit 0, no diagnostic, a
node name that is wrong. A bump would trade a stale-but-correct parser for a
current one that silently mis-parses, which is precisely the
"runtime error nobody can attribute" this issue was opened about, pointing the
other way.

There is no install target that restores it. At `07f0461e` the resolve tree has
exactly two binaries — `play_launch_parser` (no Python) and
`ros-launch-resolve/cli` (a different tool) — and `pyload` is reached only
through `resolve/`.

## So the two are LEGITIMATELY unequal today — but only for HALF the gap

The first draft of this issue said `838ce948` was "the newest commit at which
`cargo install` yields a Python-capable binary". **That was unmeasured and
false** — the same sin this issue is about, one level down, caught by going and
looking. Measured: pyo3 left in `f7f6d2cf` ("the Python half is its own
crate"), so the newest Python-capable ref is its parent **`27b6749b`**, built
and run here:

    DT_NEEDED           libpython3.10.so.1.0, libgcc_s, libc, ld
    .launch.py          /from_python
    $(eval '1 + 1')     /from_eval_2

`27b6749b` is **54 commits NEWER than the pin**, with a further 54 to the
gitlink. So the gap splits, and only the second half is actually blocked:

| segment | commits | status |
| --- | --- | --- |
| `838ce948` -> `27b6749b` | 54 | **SAFE** — a real bump, still Python-capable |
| `27b6749b` -> `07f0461e` | 54 | **BLOCKED** — the CLI loses its Python backend |

The safe half is not taken in the PR that filed this, and the reason is an
acceptance rather than a doubt: it re-cuts the dist (`0.1.0-nros2`, new sha256
per asset, re-measured floors and `system`) and `just/workspace.just` names
what must pass with it — the L.6 gate tests `phase212_l6_launch_synth::*`,
which are fixture-backed and were not run. A bump whose acceptance nobody ran
is how a pin moves wrong; that is the whole subject here.

## What would close it

In order:

* **Now, and measured**: bump `source.ref`/`upstream` to `27b6749b`, re-cut
  `play_launch_parser-0.1.0-nros2` from it, re-measure each asset's sha256,
  floor and `DT_NEEDED`-derived `system`, re-verify the `smoke` `expect`
  against the new binary, and run `phase212_l6_launch_synth::*`. Halves the
  gap and needs nothing from upstream.
* **Then, upstream**: give `play_launch_parser`'s `main.rs` a `pyload`-registered
  backend (it already exists, one crate over), and make the dist stage
  `libplay_launch_parser_pyexec.so` beside the binary — the pair shape
  `nros-launch-resolve` already ships and `launch_py_resolves_as_shipped`
  already tests. Then `source.ref` can equal the gitlink, the dist can be
  re-cut as `0.1.0-nros2`, and `system` drops `libpython310` (the new binary
  `dlopen`s an interpreter instead of naming one, which is the whole point of
  0897 — so the declaration becomes `libpython3`, the host's own, and the
  soname-exact key loses this consumer).
* **Or** decide the indexed tool does not owe `$(eval)`/`.launch.py` at all,
  bump the ref, and make the capability loss LOUD rather than silent (the
  `$(eval)` path must not exit 0 with an unexpanded substitution).

Either is upstream work in `NEWSLabNTU/play_launch`, not a re-cut here.

## Gate

`check-play-launch-parser-ref` (this issue) asserts what is true and useful
today rather than the equality that is not:

* the index's `source.ref` must be an ANCESTOR-OR-EQUAL of the recorded
  gitlink — the index may lag, never lead, and never sit on a commit that is
  not on the submodule's line;
* `upstream` and `source.ref` must agree with each other;
* while they are unequal, the index must carry a `# ref-lag:` line stating why
   — and when they become equal, that line must be DELETED. Two-way, so it
  cannot rot green in either direction.

An equality gate was considered and rejected: it would be red on the day it
landed and would have to be disabled by the first person to bump either side,
which is the one thing a gate must never be.
