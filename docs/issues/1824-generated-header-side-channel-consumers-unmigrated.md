---
id: 1824
title: "The per-build config headers still go through the `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/` side channel on every lane except Zephyr"
status: open
type: tech-debt
area: [build, cmake]
severity: medium
found: 2026-10-12
related: [phase-424, 0945, 0491, 0805, 0360, 0834, 0978, 0985, 0987, 1031]
---

## What is left

[phase-424](../roadmap/archived/phase-424-build-graph-freshness-truth.md) was
archived on 2026-10-11 with three "carried forward" items. Item 2 was superseded
by phase-429 / RFC-0090, and item 3 was closed on 2026-09-06. **Item 1 is still
open, and this issue carries it.** Item 1 is #0945 item 4: the generated
headers' consumer migration.

`nros-{c,cpp}`'s build scripts write their per-build config headers into
`$CARGO_TARGET_DIR/nros-{c,cpp}-generated/` through
`write_header_to_target_dir` (`packages/tooling/nros-build-helpers/src/shared.rs`).
That path is inside cargo's tree, but cargo does not manage it. `$OUT_DIR` is
cargo's own answer, and the Zephyr lane already uses it
(`cargo-out-dir-headers.py`). The other lanes do not.

**Size, re-measured 2026-10-12:** `git grep -n 'nros-c-generated\|nros-cpp-generated' -- ':!docs'`
gives **240 hits in 43 files**. phase-424 counted 128 hits in 26 files on
2026-09-05, so the channel has grown since it was carried forward.

## Why it is a design question, not a sweep

`write_header_to_corrosion()` already writes to `$CORROSION_BUILD_DIR`, which
cmake owns and which is outside cargo's tree. That is not enough on its own. The
target-dir copy is load-bearing because `CORROSION_BUILD_DIR` cannot be watched:
it is a path variable, and watching one as text is issue 0491's defect (issue
0805 measured ~70 spellings in one fingerprint namespace). So the Corrosion
lanes need a header destination that is safe on a cache hit, the way the Zephyr
lane got one, and Corrosion invokes cargo itself rather than being wrapped.

The risk is the one phase-424 named. The side channel has no OWNER, and that
has bitten six times (issues 0360, 0834, 0978, 0985, 0987, 1031). The
differential freshness probes cannot see it: a wrong header comes out as the
same wrong bytes twice, so the verdict is FRESH.

## What closes it

Every lane reads its config headers from a destination that cargo or cmake
owns, and the target-dir copy is deleted. The `git grep` above then returns
nothing outside the helper being retired.
