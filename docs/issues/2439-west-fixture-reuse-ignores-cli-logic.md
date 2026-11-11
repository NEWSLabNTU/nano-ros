---
id: 2439
title: "A west compile-check row is reused across a CLI change that alters `nros configure` / `nros build`, because its signature keys on the codegen fingerprint only"
status: open
type: bug
severity: medium
area: [testing, fixtures, zephyr, cli]
found: 2026-11-11
related: [phase-530, issue-2436, issue-0182, issue-1018, issue-1454]
---

## Measured

While taking issue 2436's negative control: with `nros build`'s image
statement removed from the CLI and the CLI rebuilt (`just setup-cli`),
`NROS_ZEPHYR_FIXTURE_FILTER=^west_hand_written_c_entry_nros_build bash
scripts/build/west-fixtures.sh` printed `1/1 ok (1 reused, 0 built)` and
re-stamped the old configure. Only after deleting the row's build dir did the
same mutated CLI give the real answer: the configure unseeded, refused by
`NanoRosPromise.cmake`, `MISSING build.ninja`.

So a test reading that row (`tests/west_hand_written_entry.rs`) passes on a
configure the current CLI can no longer produce.

## Why

`scripts/build/compile-check-signature.sh` names the CLI by
`nros_codegen_fingerprint`: what the tool EMITS from codegen, stable across
unrelated rebuilds on purpose (phase-318 W1). Every west row's configure also
runs the CLI's planning: the module hook calls `nros configure` (seeds, entity
inventory, message bounds), and a `west_nros_image` row runs `nros build`
itself. None of that is codegen output, so a change to it moves nothing the
signature hashes, and the reuse branch re-stamps `.compile-ok` to the new
binary (the comment there calls the fingerprint "a strictly better question
than is it the same binary"; for these rows it is a narrower one).

## Direction

The fingerprint answers "same generated code"; these rows need "same
configure answer". Either key the west rows on the CLI binary (the costlier,
honest answer), or give the CLI a second fingerprint over the planning
surface `nros configure`/`nros build` hand a configure, and hash both.
