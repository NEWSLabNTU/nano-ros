---
id: 2827
title: "A configure can read a probe sidecar before it is stamped, when another configure of the same workspace is probing, and refuses the image"
status: resolved
type: bug
severity: medium
area: [cli, sizing, census, fixtures]
related: [issue-0498, issue-2689, issue-2826]
found: 2026-11-22
resolved: 2026-11-22
resolved_in: "fix(cli, issue 2827): a probe's output reaches its sidecar only stamped"
---

## What happened

Tier 1 of scheduled run 44874748326 (2026-11-21, main at `ef47a1447b`)
failed `build-test-fixtures` in its linux stage while configuring
`connect_facts_ws`'s `cpp_bringup`:

```
CMake Error at .../cmake/NanoRosEntityInventory.cmake:676 (message):
  nros: the entity declaration in this image is not readable.

    nros: entity inventory: .../connect_facts_ws/src/cpp_listener_pkg/metadata/listener.json:
    stale against its sources (re-run `nros configure`), so neither its counts
    nor its registration observations are used.  Moved: it records no
    probe-input digest
  ...
  The image would be built on configured default sizes instead of the ones it
  declares (NROS_PROBE_FAILURE=refuse).
```

`probe_pkg/metadata/probe.json` was refused the same way at the same moment.
The full text is in that run's `tier1-junit` artifact,
`tmp/build-test-fixtures-20261121-081007-525631/linux.log` line 3222; the job
log printed only its first line (issue 2826). The same rows built clean on
main at `3d0bcacdd7`, fresh and incrementally, and no other run shows it.

## Why

A sidecar's freshness is its recorded probe-input digest, and the digest is
added by the CLI AFTER the probe runs (`stamp_probe`). Both probe roads wrote
the sidecar at its final path first:

- the C/C++ probe program writes `<pkg>/metadata/<component>.json` itself, and
  the batch stamps only after EVERY probe of the batch has run;
- the Rust harness writes it, then `relativise_source_artifacts` rewrites it
  in place, then the stamp.

So for the length of a batch, every sidecar in it sits on disk with no digest.
The fixture lane builds each workspace's units in parallel; that run started
`connect_facts_ws`'s c, rust and cpp units together (linux.log, the
`workspace unit` block). One configure re-probed: a sidecar whose digest
matches is still probed again when `build/nros-metadata` lacks the probe
build, and the log says so for `listener.json`. Another configure, whose own
refresh had found the stamped sidecars fresh, read them in that window and
found "no probe-input digest".

The stamp write itself was already atomic (issue 0498, written for exactly
this contention); the unstamped write before it was not covered.

## Fix

A probe the CLI runs writes a staging file of its own, beside the sidecar:
`<component>.json.probing.<cli pid>` (`metadata_refresh::probe_staging_path`).
The CLI hands that path to the probe as `NROS_METADATA_OUT`, which both
writers honour: the C/C++ `nros_cpp_metadata_dump` and the Rust harness.
Their compiled-in path stays the sidecar's, so the shared probe sources
don't change from run to run. That path is used only when a probe is run by
hand. Every write a probe road makes, the Rust road's path rewrite included,
lands in staging.

`stamp_probe` reads the staging file, writes the stamped sidecar atomically
and removes the staging file. So the sidecar's path only ever holds a whole,
stamped body:

- a concurrent reader sees either the previous stamped sidecar or the new
  one;
- two configures re-probing the same component never write one file, so each
  rename carries a whole body and the last one wins.

`nros dev metadata --build`, which stamps nothing, moves its output into
place as before. The name does not end in `.json`, so no `metadata/*.json`
reader lists a staging file; `**/metadata/*.probing.*` is ignored.

Tests (nros-cli-core):

- `a_probe_output_reaches_the_sidecar_only_stamped`: while the output is
  staged, the sidecar is still the previous stamped one; the stamp then puts
  the new one in place and removes the staging file.
- `two_writers_staging_one_component_leave_a_whole_stamped_sidecar`: two
  writers stage and stamp the same component 200 times each, while a reader
  polls the sidecar. It is never missing, never half a body and never
  unstamped, and no staging file is left.

## What widened the window

A sidecar whose digest MATCHES is still probed again when
`build/nros-metadata` lacks the probe build
(`a_fresh_sidecar_without_the_probe_build_is_probed_again`). The probe build
is what produces the bound tables the message-bound seeds read, so a
persistent tree that lost it re-probes components whose sidecars are current.
That turned a correct sidecar into an unstamped one for the length of a batch,
which is how a configure that had found everything fresh still read an
unstamped file.

The re-probe itself is not avoided here. Rebuilding the tables without
running the probes would need the probe project to build without writing
sidecars, a separate change. With the staging fix, a redundant re-probe
rewrites an identical stamped body and is harmless; its remaining cost is
time.

Not addressed: two configures of one workspace still build the same
`build/nros-metadata` project at once when both find it stale, which
duplicates work but no longer exposes a half-written sidecar.
