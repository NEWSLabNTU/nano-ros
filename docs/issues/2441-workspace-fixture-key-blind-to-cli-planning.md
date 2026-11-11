---
id: 2441
title: "Workspace fixtures are keyed on the CLI's codegen fingerprint, so a change to what `nros build` / `nros configure` hand a build does not re-stale them"
status: open
type: tech-debt
severity: medium
area: [testing, fixtures, cli]
found: 2026-11-11
related: [issue-2439, issue-0196, issue-1018, phase-530]
---

## What

Issue 2439's class, one lane over. `scripts/build/workspace-fixture-signature.sh`
names the CLI by `nros_codegen_fingerprint`: what its emitters produce for a
compiled-in corpus. A workspace fixture is built by `nros build`, so it is
also a function of the CLI's PLANNING: the seeds `nros configure` writes
(entity inventory, message bounds, sizing descriptor), the image statement,
the configure options. None of that is codegen output, so a CLI change to it
moves nothing the signature hashes, and the fixture is reused as fresh.

Issue 2439 fixed this for the six west compile-check rows by keying them on
the CLI SOURCE STAMP (`nros_cli_source_stamp`). That key was not applied here.

## Why not the same fix

`codegen-stamp.sh` and phase-318 W1 measured the cost of a wide CLI key on
this lane: a binary-keyed signature invalidated 40 fixtures / 35 build dirs,
a multi-hour ~100 GB rebuild, for a codegen change whose correct answer was
zero; 168 CLI binaries against 11 fingerprints. The source stamp is narrower
than a binary hash but still moves for a `doctor.rs` edit or a `play_launch`
pin bump.

## The decision

Which cost to pay:

- **Source stamp** (2439's key): correct, and pays a full workspace rebuild
  on most CLI edits.
- **A second fingerprint over the planning surface**: e.g. hash the
  `nros configure` outputs for a compiled-in corpus, as `codegen-fingerprint`
  does for emitters. Narrow and correct, but needs a corpus that exercises
  the planning paths, and is only as complete as that corpus.
- **Keep it**: accept that a planning regression can pass this lane on a
  reused build until something else rebuilds it.
