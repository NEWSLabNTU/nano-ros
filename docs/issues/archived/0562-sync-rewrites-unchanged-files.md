---
id: 562
title: "`nros sync` rewrote byte-identical files, restamping mtimes and charging a cmake reconfigure for no change"
status: resolved
type: performance
area: cli, build
related: [issue-0509, issue-0466, issue-0457, issue-0463]
---

## What

`atomic_write_bytes` (`packages/cli/nros-cli-core/src/atomic_file.rs`), the
single writer behind ~19 sync/codegen call sites in `ws.rs`,
`metadata_build.rs` and `metadata_refresh.rs`, **never compared content**. It
wrote a temp file and renamed it unconditionally, so every sync-owned file got
a fresh mtime on every run — including runs where nothing had changed.

Everything downstream of those files keys on mtime:

* cmake re-runs `configure_file` / `CONFIGURE_DEPENDS`, and on Zephyr that
  drags Kconfig, devicetree and the module scan with it;
* cargo re-fingerprints;
* the fixture staleness probes report STALE.

So a no-op `nros sync` bought a reconfigure it could not possibly need.

## Measured — no-op syncs, nothing changed between runs

| leaf | files re-stamped |
| --- | --- |
| `examples/native/rust/talker` | 2 (`build/nros/providers.json`, `.cargo/config.toml`) |
| `examples/workspaces/features` | **27** |

This is also the mechanical source of the `.cargo/config.toml` git churn that
gets discarded by hand every session: the file's *content* was stable the whole
time, only its mtime moved.

## The class, which is bigger than the one function

Fixing `atomic_write_bytes` alone moved the needle almost not at all — a no-op
sync went 27 -> 26 restamped files, and 2 -> 1. Nearly every restamped file was
written by something else, and `providers.json` proved it: its bytes were
IDENTICAL across two syncs and it was restamped anyway.

The skip-if-identical idiom already existed in the tree, in **four private
copies** — `facade::write_if_changed`, `metadata_build::write_if_changed`, an
inline check in `cmd/ws.rs`, and `model_ingest`'s hand-rolled temp+rename — and
the sites that actually cost configure work had **none** of them:

| writer | what it restamped |
| --- | --- |
| `metadata_probe_cmake::run_probes` | the probe dir's `CMakeLists.txt` + one `.cpp` per component |
| `metadata_probe_cmake::write_capabilities` | `nros_capabilities.cmake` |
| `provider_scan::ProviderIndex::write` | `build/nros/providers.json` |
| `model_ingest` | a leaf `.cargo/config.toml` |
| `cmd/ws.rs` (migration) | the consumer `Cargo.toml` |

The probe directory **is a cmake project**, so restamping its `CMakeLists.txt`
buys a probe reconfigure on every sync — the configure work this was chasing.

## Fix

One helper, in `cargo_nano_ros::atomic_file`: atomic (temp sibling +
`rename(2)`) AND write-if-changed. It lives in the LOWER crate because
`nros-cli-core` depends on it and `provider_scan` down there writes a
sync-owned file too, so one spelling can serve both; `nros_cli_core::atomic_file`
re-exports. All five sites above and all four private copies now delegate.

The atomicity property the function exists for is untouched — a reader still
sees either the old bytes or the new ones, because when they are equal there is
nothing to see. `atomic_file.rs` carries a test asserting both directions:
identical content leaves the mtime alone, a real change still lands.

## Relationship to issue 0509

0509's revised direction list leads with *"(1) skip per-leaf prep whose inputs
are unchanged"*. This is that item, at the lowest layer it can be fixed at —
one function rather than per-consumer guards.

0509 also measured the Zephyr lane at 76 % idle / 18 % iowait on an HDD with
~0 compilers live, i.e. dominated by cmake configure work. Restamped inputs are
one of the things asking for that configure work.

## Measured

Restamped files on a NO-OP sync (nothing changed between runs):

| leaf | before | one-function fix | class fix |
| --- | --- | --- | --- |
| `examples/workspaces/features` | 27 | 26 | **6** |
| `examples/native/rust/talker` | 2 | 1 | **0** |
| zephyr lane, `build/nros/**` | 5 | 5 | **0** |

The 6 that remain under `features` are inside the probe's own `build/`
directory and are written by cmake itself, not by sync.

## The wall-clock A/B is inconclusive, and the reason matters

The intended A/B was a no-op zephyr lane before and after, with a symmetric
protocol on both sides (revert-or-apply -> `setup-cli` -> warm-up run -> measured
run), because `just setup-cli` itself stales every workspace fixture (#0466) and
would otherwise be charged to whichever side rebuilt last.

It cannot answer the question. Seven no-op runs of the SAME lane, each producing
a byte-identical 1728-line log with the same 129 `Compiling` lines and the same
west pass — i.e. provably identical work — took:

```
50s  50s  51s  695s  450s  634s  630s
```

A 14x spread on identical work. On this HDD-backed host the lane's wall time is
set by page-cache state, not by what it is asked to do (0509 measured the same
host at 76 % idle / 18 % iowait, and a 60x page-cache effect on a directory walk
in phase-338). **No wall-clock claim, in either direction, is supportable from
this instrument** — which is why the restamp count above is the number this
issue rests on. It is deterministic and it is the thing the fix changes.

## Separately: the zephyr "no-op" lane is not a no-op

Every one of those seven runs replayed **1244 ninja edges** — a full Zephyr
static-library link set — plus a 129-crate cargo rebuild of `nros-c`, from the
west-fixtures step. That is unrelated to this issue and belongs to 0509's
"skip per-leaf prep whose inputs are unchanged" line; recorded there.

## Verified resolved (2026-08-15)

The class fix is in the tree and both of the issue's headline measurements
reproduce on a no-op sync (warm-up run, snapshot, second run, compare mtimes):

| leaf | this issue predicted | measured 2026-08-15 |
| --- | --- | --- |
| `examples/native/rust/talker` | 0 | **0** |
| `examples/workspaces/features` | 6 | **6** |

All six under `features` are inside `build/nros-metadata/metadata-probe-cmake/build/`
and are written by cmake itself, exactly as this issue predicted; **sync-owned
restamps are 0**. The writers that used to be unguarded —
`metadata_probe_cmake::{run_probes,write_capabilities}`, `provider_scan`,
`model_ingest`, `cmd/ws.rs` — no longer restamp their outputs.

One spelling survives, in the lower crate, with the core crate re-exporting it:

```rust
// cargo_nano_ros::atomic_file
pub fn atomic_write_reporting(dst: &Path, body: &[u8]) -> Result<bool> {
    if std::fs::read(dst).is_ok_and(|existing| existing == body) {
        return Ok(false);
    }
    ...
}
```

Gate: `check-atomic-sync-writes`.

### A measurement note, because the first attempt was wrong

The first pass used `comm` on `find -printf` output and reported 31 890
restamped files. That number was garbage — `comm` warned `file 1 is not in
sorted order` and its output on unsorted input is meaningless. The counts above
come from an explicit stat-map comparison. A tool that warns and continues will
be believed if the warning scrolls past; this issue's own thesis is that
mtime-driven work is invisible, so measuring it with a silently-wrong instrument
would have been a fitting way to close it wrongly.

### Not closed by this: the whitespace churn

`examples/threadx-linux/rust/talker/.cargo/config.toml` was observed modified
after tier-1 runs on 2026-08-14 and 2026-08-15 with a pure-whitespace diff
(`["../..` becoming `[ "../..`). That is a CONTENT difference, which
write-if-changed cannot suppress by design, so it was never this issue's to fix
— this issue's claim that "the file's content was stable the whole time" does
not hold for that leaf.

It does NOT reproduce from a direct `nros sync` of that leaf on this tree, so
either it was fixed in passing or it comes from a different writer reached only
by the full lane. Recorded in
[phase-353](../roadmap/phase-353-build-and-fixture-cost.md) W1 rather than left
inside a resolved issue.
