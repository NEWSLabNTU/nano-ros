---
id: 1970
title: "Every tier's executor is sized for the whole image, so a second tier costs a second whole-image executor"
status: open
type: enhancement
area: [memory, zephyr, codegen]
severity: medium
found: 2026-10-19
related: [rfc-0079, 1551, 1568, 1232, 0961]
---

## What happens

A tiered C/C++ entry reserves one executor block per tier,
`__nros_tier_executor_storage[n_tiers][NROS_CPP_EXECUTOR_STORAGE_SIZE]`, and
every block is the image-wide executor: the header, and an inline backing
carved at `ExecutorSizing::DEFAULT` -- the arena derived from EVERY entity the
image declares, the callback tables at the image's `MAX_CBS`, the node tables
at `MAX_NODES`, the parameter store at the image's `MAX_PARAMETERS`.

The Autoware safety island (S32K344, one tier `mrm` with three components)
measures that block at 47,296 B of DTCM (arena 25,724 B of it). Its next
step, the 30 Hz emergency operator on a thread of its own, is a second tier,
and the second tier costs a second 47,296 B block although it hosts one of the
three nodes. DTCM has 10,432 B left above the image's 8 KiB guard, so the
split cannot be built.

## Why

The tier runner's storage contract is one STRIDE (issue 1551): block `i` is
`storage + i * stride`, and the executor opened over it carves its backing
from the context's own inline `_opaque`, whose length is a property of the
TYPE (`ExecutorInlineStorage`), i.e. of the image. Nothing knows a tier's own
demand.

## Direction

- The sizing descriptor states each tier's own facts (`[[tier]]`: its nodes,
  their entity counts and callback slots, the subscriptions they register,
  their declared parameters), composed by the same entity inventory over the
  tier's members only.
- `nros-node` derives each tier's `ExecutorSizing` from those facts with the
  same arena arithmetic the image's uses, and a tiered image's contexts carry
  no inline backing.
- The generated entry gives each tier an exactly-sized backing static, the
  runner hands it to the tier's executor, and the library refuses a backing
  smaller than the tier's layout.
