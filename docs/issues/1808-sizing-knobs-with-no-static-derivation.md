---
id: 1808
title: "Five sizing knobs are still set by hand because nothing in the image can derive them"
status: open
type: limitation
area: [sizing, zephyr, memory]
severity: low
found: 2026-10-11
related: [1036, 1120, 1140, 1010]
---

## Measured

[phase-412](../roadmap/archived/phase-412-derived-counts-and-sizes.md) derived
every pool count and size the entity inventory can answer. On the safety
island (mr-canhubk344) its board `.conf` went from 324,096 B (98.91 % of 320 KB)
hand-set to 280,536 B (85.61 %) fully derived. All four acceptance items were
met on 2026-09-06, and `NROS_MAX_LIVELINESS` and the executor arena were derived
after that. The phase was archived on 2026-10-11.

What it could not derive is its W2 table. Each knob is still stated by the
operator, and each has a different blocker:

| knob | island value | blocker |
| --- | ---: | --- |
| `CONFIG_MAIN_STACK_SIZE` | 16384 | Needs frame analysis, not an inventory. phase-409 has the method (`objdump`, summing every `sub sp`) for one call chain; nothing turns it into a knob. |
| `NROS_ZEPHYR_HEAP_SIZE` | 94208 | Runtime allocation, no static model. The first step is a high-water reporter. Exhaustion is no longer silent (`nros: HEAP EXHAUSTED: ...` since issue 0968; issue 1010 is that message in the field). |
| `NROS_GRAPH_CACHE_SIZE` | 4096 | Sized by the peer graph, which is not a property of this image. Delivery on every lane was fixed 2026-09-08; the value stays operator-stated. |
| `NROS_ZEPHYR_TASK_SLOTS` | 5 | Transport tasks, not entities. Derivable in principle from the transport's own declaration; nothing declares it. |
| `NROS_ZEPHYR_TASK_STACK_SIZE` | 8192 | Same as above. |

The island's board `.conf` carries each blocker as a comment beside the value.

## Related work that is not this issue

- The sizing knobs not yet on the RFC-0049 ladder (11 on 2026-10-11, from
  `python3 scripts/check/config-knob-census.py`) belong to
  [phase-400](../roadmap/phase-400-unified-configuration.md).
- Two issues phase-412 homed stand on their own: 1120 (the inventory's refusal
  names the retired `ENTITIES`) and 1140 (two "was a contract authored?"
  predicates disagree on a timer-only contract). A third, 1036, is resolved.

## Closes when

Each row above is either derived from something the image declares, or recorded
as permanently operator-stated with its reason in the knob's own help text. The
graph cache is likely the second kind.
