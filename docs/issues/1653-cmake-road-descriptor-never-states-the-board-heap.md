---
id: 1653
title: "No cmake or west entry's sizing descriptor states the board heap, so RFC-0100 D11's Cyclone heap assertion is inert on exactly the roads embedded Cyclone images build on"
status: open
type: tech-debt
area: [build, cmake, rmw]
severity: low
found: 2026-10-03
related: [1393, 1649, rfc-0100, rfc-0065, phase-457]
---

## What

RFC-0100 D11 has Cyclone compare the heap an image is CONFIGURED with
(`[target].heap_budget_bytes`, from the board's `[board.knobs.memory]
heap_bytes`) against the floor it derives from `[types]`, and fail the boot when
the heap is short. The C++ side reads the budget as
`NROS_CYCLONEDDS_HEAP_BUDGET_BYTES` (`heap_budget.hpp`); with it undefined,
`kHeapBudgetStated` is false and the check is a no-op by design (D6: "nobody
said" is not "too small").

Only the cargo roads ever define it:

* `nros build` writes the board heap into a workspace cargo image's descriptor
  (`cmd/build.rs`, `board_heap_budget`), and `WrittenDescriptor::cyclonedds_env`
  turns it into the `[env]` row `nros-rmw-cyclonedds-sys/build.rs` forwards;
* the cargo leaf road does the same through `cmd::leaf_settings`.

The cmake road never does. `nros ws sizing-descriptor --from-model` has the seam
(`--heap-budget-bytes`, "when the caller knows it"), but nothing in `cmake/` or
`zephyr/cmake/` passes it, and `HEAP_BUDGET` appears in no `.cmake` file or
`CMakeLists.txt` (measured 2026-10-03: `git grep -n HEAP_BUDGET -- '*.cmake'
'*CMakeLists.txt'` is empty). phase-457 W4 recorded this half on 2026-09-28 —
"nothing on the cmake road resolves a board heap knob at all" — and left the
field refused with that narrower reason; no issue carried it afterwards.

## Why it matters more than one refused field

Embedded Cyclone images build on the cmake and west roads (the C/C++ path is
where `nros_find_interfaces` emits the descriptors Cyclone needs, and every
Zephyr image is a west build). So the one boot check this RFC built for the
failure mode it names — Cyclone exhausting the ddsrt heap inside entity creation,
dying as an anonymous `abort()` twenty seconds later (issues 0371 / 0496) — is
live on the host-side cargo images, where the heap is the host's, and inert on
the RTOS images that have a fixed heap.

## Direction

The board is a fact `nros build` resolves for every road at RFC-0065 stage 2/4
(it already reads `board_heap_budget` for the cargo road); the cmake configure
does not resolve board knobs and should not learn to. Stage 4 emits the cmake
root, so it can hand the resolved `heap_bytes` to the configure the same way it
hands the triple, and `nano_ros_entry()` passes it on to `--heap-budget-bytes`.
The west road emits no root (RFC-0065 D3), so its value has to come through the
generated west application or the module's knob resolver — the same choice
issue 1407's fix made for the descriptor path itself.

Do NOT make cmake read `nros-board.toml` directly: that is a second reader of
the board's knobs, the shape RFC-0064 R5 D4 removed.

## Acceptance

A cmake Cyclone image (e.g. a `examples/workspaces/cpp` Cyclone cross row) whose
board states `heap_bytes` writes a descriptor with `[target].heap_budget_bytes`
stated, the Cyclone TU compiles with `kHeapBudgetStated == true`, and a board
heap set below `kRequiredHeapBytes` fails at boot with D11's message (the
non-default probe, `canonical-build-path.md`'s rule) — and the same on one west
image.
