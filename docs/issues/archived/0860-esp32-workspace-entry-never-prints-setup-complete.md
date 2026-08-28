---
id: 860
title: "The ESP32-C3 workspace Entry never reaches `Application setup complete`
  — a MUSEUM BINARY built 90 minutes before issue 0857's fix landed"
status: resolved
resolution: duplicate
duplicate_of: 857
type: bug
area: boards, rmw
related: [phase-391, issue-0857, issue-0851, issue-0445]
---

## Resolution: duplicate of issue 0857, and the report was filed off a stale artifact

`test_esp32_workspace_entry_e2e` did not hang. It PANICKED, and the panic names
the cause outright once the boot log is read to the end rather than to the 900
characters the first look truncated it to:

```
Ethernet ready.
====================== PANIC ======================
panicked at .../alloc/src/alloc.rs:573:9:
memory allocation of 17468 bytes failed
```

17,468 bytes is the cap-8 `Arc<ComponentCell>` that **issue 0857** is about —
the phase-391 W5 inline cell registries costing `CELL_REG_CAP × ~1.35 KiB` per
component, which OOMs the 48 KiB esp-alloc heap on the second component. 0857
fixed it for this fixture by pinning `NROS_RUNTIME_MAX_CELL_ENTITIES = "2"` in
the row's `env` (commit `2ef7ae1e9`).

The timestamps are the whole story:

| event | time |
| --- | --- |
| the esp32 image under test was built | 2026-08-28 **03:19:08** |
| 0857's knob landed in `examples/fixtures.toml` | 2026-08-28 **04:48:54** |

The image predates its own fix by 90 minutes. Rebuilding the row regenerates
`nros_runtime_config.rs` with `MAX_CELL_ENTITIES = 2` (verified), which is the
constant whose value 17,468 bytes was a direct function of.

## Two wrong turns, recorded because both were nearly acted on

**"The knob cannot be reaching the build."** `packages/api/nros/build.rs` reads
four `NROS_RUNTIME_*` knobs and emits no `cargo:rerun-if-env-changed` of its
own, which looked like a fingerprint bug that would make the knob permanently
inert. It is not: `knob_usize` emits the directive itself
(`nros-zephyr-build/src/lib.rs:116`), and the rebuild picking up `2` proves the
mechanism works end to end. A "fix" adding those directives would have changed
nothing and buried the real cause.

**"Start at W2's allocator replacement."** The original filing reasoned that
because 0851 unblocked compilation for `riscv32imc`, this was the first runtime
look since phase-391 W2 replaced the allocator, and pointed the next person at
rlsf. That framing is sound and the conclusion is still wrong — the heap is
fine, the thing asking it for memory was too big.

## The rule this cost

A fixture built before a fix reports the bug the fix removed, and it reports it
in full detail, which is what makes it convincing. The staleness gate exists to
force this check and I filed around it: the run that produced this failure used
fixtures built at 03:19 against a tree rebased past 04:48.

**Before filing from a failing fixture, compare the artifact's mtime against
the head of the tree that is being blamed.** Sibling issues 0859, 0861 and 0862
came out of the same pre-rebase run and need the same check before any of them
is trusted. → issue 0445, and the museum-binary class in CLAUDE.md.

## Still open, and owned by 0857

The cap-2 pin is explicitly interim: "until the W5-endgame per-class exact
cells retire the worst-case pad". A component here declaring more than two
entities of one kind still fails registration by design. That is 0857's
remaining work, not a separate issue.
