---
id: 900
title: "Every executor arena slot is budgeted at the ActionClient worst case, so a pub/sub-only image carries ~56 KiB it cannot use"
status: open
area: core, memory
severity: medium
found: 2026-08-29
related: [0896, 0271, 0739, phase-392]
---

# The arena is sized for the entity an image does not have

## What was measured

`ARENA_SIZE` is **74,240 bytes on every generated config in the tree**, without
variation:

| image | `MAX_CBS` | `DEFAULT_RX_BUF_SIZE` | `ARENA_SIZE` |
| --- | ---: | ---: | ---: |
| `threadx-linux/rust/talker` | 4 | 1024 | 74,240 |
| `workspaces/c` (xrce) | 4 | 1024 | 74,240 |
| `workspaces/realtime-c` (nuttx riscv32imac) | 4 | 1024 | 74,240 |
| `workspaces/realtime-cpp` (nuttx riscv32imac) | 4 | 1024 | 74,240 |

The first row is a TALKER. It publishes on a timer and owns no action client,
no action server, and no service. It carries the same arena as everything else,
on a 32-bit embedded target as on a host.

## Why

`nros-node/build.rs:90-135` derives the arena by budgeting EVERY slot at the
largest entity that could occupy one:

```rust
const ACTION_CLIENT_PER_SERVICE:   usize = 4096 + 384;
const ACTION_CLIENT_SERVICES:      usize = 3;
const ACTION_CLIENT_FEEDBACK_SUBS: usize = 3;
const ACTION_CLIENT_SUB_OVERHEAD:  usize = 1536;
const ARENA_BASE_OVERHEAD:         usize = 2048;

let per_entry = ACTION_CLIENT_SERVICES * ACTION_CLIENT_PER_SERVICE
              + ACTION_CLIENT_FEEDBACK_SUBS * rx_buf_size
              + ACTION_CLIENT_SUB_OVERHEAD;
let derived_arena = (max_cbs * per_entry + ARENA_BASE_OVERHEAD).max(ARENA_FLOOR);
```

At the defaults: `per_entry` = 14,976 + 3·1024 = 18,048, and
`4 × 18,048 + 2,048` = **74,240**. The build script says so itself:

> Subscription / service entries are strictly smaller, so budget every slot at
> the action-client size.
>
> Embedded targets that never instantiate an `ActionClient` can override the
> derived size with `NROS_EXECUTOR_ARENA_SIZE`. A pub/sub-only workload only
> needs `3 × rx_buf + 512` per entry.

Taking that note at its word, a pub/sub-only image needs
`4 × (3·1024 + 512) + 2,048` = **16,384** bytes. The difference is
**57,856 B (~56.5 KiB)** carried by every image with no action client — which
is most of them.

## Two things make this worse than a loose default

**The rx buffer is amplified 12x.** `rx_buf_size` enters `per_entry` THREE
times (goal/result/feedback), and `per_entry` is multiplied by `MAX_CBS`. So
`NROS_SUBSCRIPTION_BUFFER_SIZE` is charged `3 × MAX_CBS` = 12x into the arena at
the defaults, on top of its per-subscription cost. Anyone raising that knob to
fit one large message type pays for it twelve times over here. That coupling is
also why issue 0896's per-type receive sizing cannot help this number: the arena
slot is sized from the GLOBAL knob regardless of what any individual
subscription needs.

**The escape hatch is a knob nobody can find.** `NROS_EXECUTOR_ARENA_SIZE`
exists, is honoured, and has a Kconfig sentinel — but nothing computes a right
value for an image, nothing warns when the derived one is 4x what the image
uses, and the correct replacement (`3 × rx_buf + 512` per entry) appears only in
a build-script comment. That is the shape of issues 0271 / 0739: "a knob nobody
can enumerate is a knob nobody sets", which cost ~145 KB in one image.

## Where the bytes actually land — NOT yet attributed

The arena is carved from `ExecutorStorage`'s backing
(`executor/storage.rs:38`), and how that backing is provided differs by path:

* the C API places it in the caller's `nros_executor_t._opaque`
  (`EXECUTOR_OPAQUE_U64S`), so a file-scope executor lands it in `.bss`;
* the ThreadX board takes it from the byte pool — `nm` on
  `threadx-linux/rust/talker` shows no arena symbol at all, only
  `byte_pool_storage` at 4 MiB, so the cost is pool consumption rather than a
  named static.

So the 74,240 figure is EXACT as a configured size and verified across four
builds, but its RAM attribution is per-platform and unmeasured. **Run
`just mem-report --json --baseline` before and after any fix rather than
quoting the derivation** — phase-392's own rule, and the reason issues 0148 /
0164 were filed from numbers that did not survive a clean rebuild.

## Direction

Size a slot by the entity kind that will occupy it instead of by the worst kind
that could. Registration already knows the kind, and `system.toml` / the
`SystemModel` know the entity inventory ahead of the build for images that
declare one. Two obvious shapes, not yet chosen:

1. **Per-kind slot budgets** — the arena becomes a sum over declared entities
   rather than `MAX_CBS x worst_case`. Needs the inventory at build time.
2. **A derived cap plus a diagnostic** — keep the worst-case derivation, but
   report at boot (or fail the build) when the configured arena exceeds what the
   registered entities can use, so the existing `NROS_EXECUTOR_ARENA_SIZE`
   override becomes actionable rather than folklore.

(2) is strictly cheaper and does not need the model; (1) is the actual fix.

## Not to be confused with

Issue 0896, which is about a SUBSCRIPTION's receive buffer taking the small size
class because nothing states a per-type bound. This is one level up: even with a
perfect per-type hint, the arena slot holding that subscription is still
budgeted as though it were an action client. The two share the
`NROS_SUBSCRIPTION_BUFFER_SIZE` knob and nothing else.
