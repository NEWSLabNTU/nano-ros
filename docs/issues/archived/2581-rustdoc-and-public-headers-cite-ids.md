---
id: 2581
title: "Public API docs (`pub` rustdoc, module docs, installed headers) cite internal ids"
status: resolved
type: tech-debt
severity: low
area: [ux, docs, api]
related: [issue-2305, issue-2443, issue-2551, issue-2554, phase-523]
found: 2026-11-15
resolved: 2026-11-18
resolved_in: "the ratchet's PR, then sweeps 0dc1d9017b, 8c56e6ef6a, 08cd53592d, 9a64744509, 1037efabd0, PR #4248 (platform and core) and its follow-up (rmw backends, boards, drivers, runners and the rest)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md) W5.**

## What it was

User ruling A (2026-11-12) keeps phase, issue and RFC ids out of
user-reachable text, and the 2026-11-14 ruling applies it to public API
docs: the `///` lines of a `pub` item, every `//!` line, and the comment
text of every installed header. Private items and plain `//` comments may
keep ids. Measured on 2026-11-15: 7,631 ids in 555 files.

## Resolution

`check-user-surface-ids` reads those docs in every crate an app links
(everything under `packages/` except `cli/`, `tooling/` and `testing/`),
through a per-file counted ratchet in `.config/api-doc-ids-ratchet.json`
that may only fall. The sweeps took it to **6**, user API first (`nros-c`
and `nros-cpp` headers, `nros`, `nros-node`, the orchestration IR, the
zenoh RMW), then the platform and core crates, then everything else. Where
a sentence leaned on an id it now states the fact; a bare citation was
dropped. Each sweep changed comment text only, and regenerated the bindgen
copy of the header docs where it applied.

**The last 6 went with issue 2554.** All 6 were the "(phase-429 W1)" in
the `lib.rs` rustdoc of the six tracked crates under
`packages/interfaces/generated/humble`, which had drifted from their
emitter, whose own text was already clean. [Issue
2554](2554-generated-humble-drifts-from-emitter.md) regenerated them on the
pinned Humble, and the ratchet is now empty (`{}`): every file is held at
zero. Dropping the ratchet file for a plain baseline-zero rule, as for the
other subjects, is a follow-up to the check, not to the docs.

Stated gaps of the reading, unchanged: `/** */` docs in Rust, and a `pub`
item inside an inline `#[cfg(test)]` module. Internal references the id
pattern does not match (a bare wave name such as "W4b", "the #38 gate")
were fixed where the sweeps met them, but no gate reads them.
