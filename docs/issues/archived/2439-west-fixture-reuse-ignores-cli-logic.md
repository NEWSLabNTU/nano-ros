---
id: 2439
title: "A west compile-check row is reused across a CLI change that alters `nros configure` / `nros build`, because its signature keys on the codegen fingerprint only"
status: resolved
type: bug
severity: medium
area: [testing, fixtures, zephyr, cli]
found: 2026-11-11
related: [phase-530, issue-2436, issue-0182, issue-1018, issue-1454, issue-0196, issue-2441]
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

## Resolution

Resolved 2026-11-11, for the west rows. Their key is now the CLI's SOURCE
STAMP, on both sides, through one helper:

- `nros_cli_source_stamp` (`scripts/build/codegen-fingerprint.sh`) reads
  `nros source-stamp`: the stamp over the sources on disk now (issue 0627's
  generated closure).
- Build side: `compile-check-signature.sh` hashes it for `west-build` /
  `west-configure` rows, so a CLI source change invalidates the `.inputsig`
  and the reuse branch no longer fires. `west_fixture_stamp` also writes it
  into `.compile-ok` (`tool:nros-source-stamp=`).
- Test side: `require_west_fixture` compares that line against the same
  helper's answer now (`cli_source_stamp_now`, shelled like issue 1454's
  resolver identity), instead of the binary's sha256, which the reuse branch
  re-stamped and so vouched for whatever the signature let through (issue
  0196: the two sides now watch one input).

Negative control, measured on `west_hand_written_c_entry{,_nros_build}`: one
comment line appended to `nros-cli-core/src/cmd/configure.rs`, no CLI
rebuild. Both tests fail `STALE -- built from other nros CLI sources (source
stamp 13f141d27bd62975 in the stamp, 383f0b523b4e4214 now)`, and the lane
does not reuse the row (it re-configures, which the stale CLI then refuses).
Restored: the lane reuses again and both tests pass.

Wider than needed on purpose: a `doctor.rs` edit or a `play_launch` pin bump
moves the stamp too. That costs the six west rows a re-configure (two of them
a full build), the safe direction and cheap at this size.

Not changed: the WORKSPACE lane (`workspace-fixture-signature.sh`) keys on the
codegen fingerprint too, and its rows run `nros build` / `nros configure` as
well, so it has the same blind spot. It is not keyed on the source stamp
deliberately: `codegen-stamp.sh` and phase-318 W1 measured what a wide CLI
key costs there (40 fixtures, ~100 GB of rebuilds for a change that moved no
output). That trade is issue 2441.
