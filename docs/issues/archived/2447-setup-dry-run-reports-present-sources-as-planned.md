---
id: 2447
title: "`nros setup --dry-run` reports a present submodule source as \"would provision\""
status: resolved
type: bug
area: cli, setup
purpose: [industrial, education]
related: [phase-551]
found: 2026-11-11
resolved_in: "setup-dryrun-present branch: each submodule arm's dry run asks the presence predicate its real run asks"
---

**Home: [phase-551](../../roadmap/phase-551-starter-image.md)** — W7 checks guarantee 2 (the baked store matches the release) with `nros setup --dry-run`, so the dry run has to tell present from absent.

## Measured

In the W2 starter image (release `0.6.0-nros8`, `mps2-an385-freertos` provisioned at build time), `nros setup mps2-an385-freertos --rmw zenoh --dry-run` listed `freertos-kernel`, `lwip`, `zenoh-pico` and `mbedtls` as `[would provision (--dry-run)]`. A real `NROS_OFFLINE=1 nros setup` of the same board said `[already present (skip)]` for all four. Main behaved the same way on a provisioned host.

## Cause

`provision_source_unlocked`'s submodule arm returned `SourceDisposition::Planned` before any presence check. The clone arm had always checked first. The tools rows were not affected.

## Resolution (2026-11-11)

The early return is gone. Each of the three submodule arms asks, on a dry run, the same predicate its real run asks, and stops before the first side effect:

- store location: `store_source_present`, the provenance commit;
- installed root with a recorded pin: `recorded_pin_present`;
- checkout: `submodule_is_present`.

The first two predicates were extracted from `materialise_into` and `provision_at_recorded_pin`, so each test exists once. The test `a_dry_run_reports_a_present_submodule_source_as_present` covers the installed and checkout arms, absent then present, and a drifted checkout. It also asserts the dry run runs no git verb that writes. It fails when the dry run answers "Planned" unconditionally. The store predicate is asserted in the materialise test.
