---
id: 1917
title: "`init_with_launch_auto` documents a `NROS_RUNTIME_OVERLAY` sidecar written by `nros launch --emit-runtime-overlay`, and neither the verb, the flag, nor any reader of the variable exists"
status: open
type: tech-debt
area: [api, docs]
severity: low
found: 2026-10-15
related: [0039, rfc-0089, phase-212, phase-499]
---

## Measured

The launch-aware init entry points describe a three-rung overlay ladder, in
Rust (`packages/api/nros/src/init.rs`, module docs at "Launch overlay" and the
doc comment on `init_with_launch_auto`) and in C++
(`packages/api/nros-cpp/include/nros/node.hpp`, `init_with_launch_auto`):

1. `$NROS_RUNTIME_OVERLAY` — a JSON sidecar "written by
   `nros launch --emit-runtime-overlay`" (marked NOT yet consumed);
2. `<CARGO_MANIFEST_DIR>/launch/*.launch.xml`, parsed in-process (marked NOT
   yet parsed);
3. the env vars "the parent `nros launch` process exports"
   (`ROS_DOMAIN_ID`, `NROS_LOCATOR`, …) — "the de-facto launch overlay today".

Checked against the tree on 2026-10-15:

- **There is no `nros launch`.** `nros --help` lists no such verb, and
  `nros launch` answers `unrecognized subcommand 'launch'` (nearest: `plan`).
  So rung 1's producer does not exist, and rung 3's "parent launcher" is not a
  process anything starts — the env vars are whatever the operator set.
- **Nothing reads `NROS_RUNTIME_OVERLAY`.** Every hit outside docs is a
  comment: the two TODO blocks, the five C++ `service-client` examples' "no
  `$NROS_RUNTIME_OVERLAY` / launch XML is in scope" line, and
  `tests/init_api.rs`'s module note. RFC-0089 and the archived phase-499
  table nevertheless classify it as a kept "build/test fact".
- **`InitError::LaunchParseFailed` is never constructed** ("Phase 212.L.5
  ships a stub … Until then this variant is unused"), and
  `init_with_launch(path)` only checks that `path` exists.
- The only test of the overlay was an empty-bodied `#[ignore]`d placeholder,
  `init_api::nros_init_with_launch_auto_applies_xml_params`, deleted in
  PR #2247 (it would have passed, un-ignored, having asserted nothing). With
  it went the last thing that tracked the follow-up — the "follow-up wave" is
  phase-212 L.5, archived.

## Why it matters

How launch content reaches an image TODAY is the build-time road: `nros sync`
resolves the launch tree into a SystemModel, entries name it
(`nros::main!(launch = …)`, `nano_ros_entry(LAUNCH …)`), and per-process
`--ros-args` remaps/params are honoured at init. The init docs describe a
different, runtime-launcher architecture, so a user reading them looks for a
verb and a variable that do not exist, and a contributor reading the TODOs
thinks a sidecar format is half-built.

## What a fix decides

Either the runtime overlay is still wanted — then it needs an owner and a
design (RFC), since its producer was a launcher process nano-ros never grew —
or it is retired, which is one change:

- rewrite the Rust and C++ `init_with_launch*` docs to describe what they do
  (env vars + `--ros-args`; `init_with_launch(path)` checks existence), and
  point at the build-time SystemModel road for launch params;
- delete the two TODO blocks and `InitError::LaunchParseFailed` (a public
  enum variant: note it in the changelog);
- drop `NROS_RUNTIME_OVERLAY` from RFC-0089's kept-knob list and from the
  five examples' comments.
