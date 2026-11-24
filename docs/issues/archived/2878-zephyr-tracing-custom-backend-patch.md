---
id: 2878
title: "Zephyr 3.7 cannot select an out-of-tree tracing backend, so a consumer with its own backend carries a Zephyr patch nano-ros's patch set does not"
status: resolved
type: design-question
severity: low
area: [zephyr, tracing, boards]
related: [phase-478, 2766, 2641, 2877]
found: 2026-11-23
resolved: 2026-11-24
resolved_in: "feat(zephyr): an image selects its own tracing backend (issue 2878)"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Ruling (2026-11-24): answer 1, nano-ros carries it

nano-ros carries the patch for both lines, as a general facility: any image
can name its own backend, and the island's ring is one user. The second answer
was not taken: it would have left every consumer with its own re-apply step
after `nros setup board`, which is the shape 2766 removed.

4.4 needs it too. Its chain is gone (`tracing_core.c` reads
`CONFIG_TRACING_BACKEND_NAME`), but that string has no prompt and only
in-tree defaults, and the backend choice has no member an out-of-tree backend
can take, so an image still cannot select its own backend without a patch.

## Fix

- `scripts/zephyr/tracing-custom-backend-patch.sh <workspace>`, run by both
  `scripts/zephyr/patches/3.7.sh` and `4.4.sh` (so by `nros setup board`
  and `just zephyr setup`). Same two symbols on both lines, the island's
  names: `CONFIG_TRACING_BACKEND_CUSTOM` (a member of the backend choice) and
  `CONFIG_TRACING_BACKEND_CUSTOM_NAME` (a string, depends on it). 3.7: one
  `#elif` in `tracing_core.c`; 4.4: one `default` of
  `TRACING_BACKEND_NAME`, `tracing_core.c` untouched. Idempotent, with the
  2831 check mode.
- Gate `just check zephyr-tracing-custom-backend-patch`
  (`tests/zephyr-tracing-custom-backend-patch-tests.sh`): synthetic 3.7 and
  4.4 trees; check mode, apply, idempotent, half-applied and anchorless trees
  refused, both sets run it.
- Live witness `just zephyr test-tracing-custom-backend`
  (`tests/zephyr-tracing-custom-backend/`): a plain `native_sim` app
  registers `nros_test_ring` with `TRACING_BACKEND_DEFINE()`, selects it by
  the two symbols, and the CTF stream it records holds the stock
  `thread_name_set` event the app caused (thread id + name). Run 2026-11-24
  on the store workspaces: 3.7 PASS (422 CTF bytes), 4.4 PASS (618 bytes).
  On an unpatched tree the recipe refuses by name before building.
- Gate `zephyr-module-binding` exempts the app (plain Zephyr, no nros module); its EXEMPT takes a list per file now.
- Book: [Trace callbacks](../../../book/src/guide/realtime/trace-callbacks.md),
  "A Zephyr tracing backend of your own".

## What the island changes to drop its patch

Once its nano-ros pin carries this fix:

- delete `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch`
  (the last one, so `patches/zephyr/` goes) and
  `scripts/apply-zephyr-patches.sh`, and its calls in
  `scripts/bootstrap-asi.sh`, `build.sh`,
  `.github/scripts/run-zephyr-fvp-ci.sh` and `run-zephyr-fvp-tap-ci.sh`;
- `prj-trace-ring.conf` stays as it is: the symbol names are the same;
- in the SAME change as the pin bump: after `nros setup board`, the tree
  already carries the edit, so the island's `git apply` of 0003 neither
  reverse-applies nor applies and the script fails the build.

