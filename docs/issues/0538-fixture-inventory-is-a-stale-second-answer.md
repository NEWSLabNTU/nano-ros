---
id: 538
title: "fixture-inventory.py claims to list the fixtures outside the manifest, has no consumer, and is wrong in both directions"
status: open
type: tech-debt
area: build, testing
related: [issue-0535, issue-0537, phase-226, phase-344]
---

## Problem

`scripts/build/fixture-inventory.py` advertises itself as the answer to "which
fixture builds are not in the manifest":

> * the small set of hand-authored recipe leaves that are not yet in the fixture
>   manifest.

It is a phase-226.A diagnostic. Phase 226 is archived, and the script has **no
consumer**: `grep -rn fixture-inventory` over `justfile just scripts packages
.github` returns hits only in archived roadmap/issue prose. Nothing builds,
tests, or gates against it, so nothing has ever failed when it drifted — and it
has drifted in both directions.

### Wrong: 3 of its 5 hand-authored rows now have manifest rows

`hand_authored_rows()` (`fixture-inventory.py:522`) claims these are outside
`examples/fixtures.toml`:

| row | claim | reality |
| --- | --- | --- |
| `qemu-smoltcp-bridge` | "not covered by examples/fixtures.toml" | row at `fixtures.toml:1669` |
| `native-rust-cyclonedds-talker` | "pure-cargo Cyclone lane outside manifest" | row exists (`list --platform linux --lang rust --rmw cyclonedds`) |
| `native-rust-cyclonedds-listener` | same | same |
| `threadx-riscv64-rust-talker-cyclonedds` | "gated helper `build_threadx_cmake_rmw`" | row at `fixtures.toml:3625`, added by phase-344 W2 **for this exact reason** |
| `esp-idf-smoke` | outside the manifest | still true |

Phase-344 W2's row even carries the comment explaining why it was added
("Without a row its output was unattributable and `row_artifact_root()` named a
`target/` dir nothing writes") — the migration happened and the inventory was
never told.

### Wrong: it misses most of what it claims to enumerate

Absent from the inventory entirely:

* the 70 `zephyr-fixture-leaves.sh` leaves (it *has* a `zephyr_rows()` reader,
  but treats them as a first-class source, never as manifest-less debt);
* the 4 `west-fixtures.sh` fixtures;
* all 4 `build-fvp-*` artifacts (issue 0537);
* the esp32 `build/esp32-qemu/*.bin` espflash postprocess;
* the `ros-editions` fixtures.

## Why this is worse than having no inventory

An unmaintained list that answers the right question is read as authoritative
exactly when someone is auditing coverage — which is the moment a false negative
costs the most. Its `--summary` is cited four times in phase-226's acceptance as
"the first thing to run".

## Direction

One of two, not both:

1. **Retire it.** Delete the script and the archived-doc pointers. The
   manifest + issue 0535's migration make it redundant by construction: once the
   74 west fixtures have rows, "outside the manifest" is a set the manifest
   itself can answer.
2. **Gate it.** Keep it, make `hand_authored_rows()` assert that each entry is
   genuinely absent from `fixtures.toml` (a stale entry FAILS, like
   `examples_fixture_coverage.rs`'s stale-exception arm), and wire it into
   `just check`.

Option 1 is preferred and should follow issue 0535. Option 2 is the fallback if
0535 stalls — an ungated list is the one outcome to not keep.
