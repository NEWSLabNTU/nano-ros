---
id: 2878
title: "Zephyr 3.7 cannot select an out-of-tree tracing backend, so a consumer with its own backend carries a Zephyr patch nano-ros's patch set does not"
status: open
type: design-question
severity: low
area: [zephyr, tracing, boards]
related: [phase-478, 2766, 2641, 2877]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I38. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

On Zephyr 3.7, `subsys/tracing/tracing_core.c` picks the backend's name with
an `#elif` chain over the in-tree `CONFIG_TRACING_BACKEND_*` symbols.
`TRACING_BACKEND_DEFINE()` is public, so an application can register a
backend, but it cannot select it: the name is a compile-time constant of the
chain. The island's overwrite-oldest RAM ring
(`actuation_module/src/asi_common/src/common/diag/trace_ring_backend.c`,
`CONFIG_ASI_TRACE_RING`) is such a backend.

Issue 2766 moved the island's other two Zephyr patches (the CTF `app_marker`
event and the `k_thread_priority_set` mapping) into nano-ros's patch set
(`scripts/zephyr/ctf-app-marker-patch.sh`, `ctf-priority-set-patch.sh`). It
left this one out on purpose: "nothing in nano-ros needs it, and its exit plan
retires it on 4.x". So the island still carries one Zephyr patch, which is
what keeps it from building against any Zephyr tree but its own (issue
2877).

## Island workaround

- `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch`: adds
  `CONFIG_TRACING_BACKEND_CUSTOM` and `CONFIG_TRACING_BACKEND_CUSTOM_NAME` to
  `subsys/tracing/Kconfig` and one `#elif` to `tracing_core.c`.
- `scripts/apply-zephyr-patches.sh`, run by `scripts/bootstrap-asi.sh` and by
  every `build.sh` Zephyr build, applies it idempotently after `nros setup
  board` rewrites `zephyr/`.
- `actuation_module/src/controller_bringup/zephyr/prj-trace-ring.conf` selects
  it (`CONFIG_TRACING_BACKEND_CUSTOM=y`,
  `CONFIG_TRACING_BACKEND_CUSTOM_NAME="asi_trace_ring"`).

## The question

One of:

1. nano-ros carries the patch for the 3.7 line beside the 2766 scripts
   (`scripts/zephyr/tracing-custom-backend-patch.sh`, applied by `nros setup
   board`), as a general facility: any image can name its own backend. It
   is a Kconfig choice member and one `#elif`; 4.4 needs it too unless
   upstream's chain changed.
2. nano-ros declines again, and the book says how a consumer carries a Zephyr
   patch of its own against a nano-ros-provisioned tree (which tree, when it
   is re-applied after `nros setup board`), so the island's script is the
   documented shape, not a workaround.

## Acceptance

- The ruling is recorded here.
- Under 1: a `native_sim` Zephyr 3.7 image with a backend registered by
  `TRACING_BACKEND_DEFINE()` outside the tree and selected by the two symbols
  emits its CTF stream through it; the island deletes `patches/zephyr/0003`
  and `scripts/apply-zephyr-patches.sh`.
- Under 2: the book page exists and the island's script follows it.
