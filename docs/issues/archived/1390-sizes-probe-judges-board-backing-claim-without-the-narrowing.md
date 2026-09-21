---
id: 1390
title: "The sizes probe inherits a board's executor-backing CLAIM but not the
  image's knob narrowing, so it judges the claim against a default belonging to
  no image — which forces every board claim up to the unnarrowed maximum"
status: wontfix
closed: 2026-09-21
type: bug
area: [build, core]
severity: medium
found: 2026-09-20
related: [1388, 1145, 1171, 1284, 1402, 0460]
---

## What happens

`nros-c` / `nros-cpp`'s size probe (`nros-sizes-build`) spawns a NESTED cargo
build to measure the executor's layout for the generated C header. That build
compiles `nros-node`, which resolves the backing knob with
`env_opt_usize_laddered("NROS_EXECUTOR_BACKING_U64S")`:

1. the env var — **absent** in the nested build;
2. otherwise the RFC-0049 ladder, which reads `NROS_BOARD_TOML` — **present**,
   because the probe inherits it.

So the probe picks up the BOARD's `[board.knobs.executor] backing_u64s`. The
image's SIZING knobs do not arrive that way: they come from the entity
inventory through the process env, and that env does not reach the probe.

**Measured** (threadx-linux, 2026-09-20, by dumping the build script's own
environment before the spawn): **56 `NROS_*` variables, none of them a sizing
knob** — only `NROS_DECLARED_NODES=3` and `NROS_DECLARED_INFRA_QUERYABLES=none`
alongside paths and `NROS_KIND_*` tags.

The result is that `nros-node`'s const assertion — `stated >= default` — rules
inside the probe on a pairing that exists in no image: the board's statement
against an UNNARROWED default. On threadx-linux that was 4494 against 11069,
while the image's own defaults are 2917 / 3626 / 4494 and the statement was
correct for them.

Two rungs of one ladder arriving by different routes, only one of which
survives the process boundary.

## Why it matters even though issue 1388 is fixed

1388 raised the claim to 11069 — the unnarrowed maximum — for an independent
reason: the synthesised `nros_ws_runtime` umbrella is compiled per CARGO ROOT,
takes the unnarrowed default too, and is part of the ordinary build. So the
build is green today and this probe behaviour is currently masked.

What it costs is the OPTION. As long as the probe judges a board's claim
without the narrowing, no board can ever state a per-image-correct number —
every claim is forced up to the unnarrowed maximum, and the rung's whole
purpose (state what THIS image reserves, so the pool gives back exactly that)
is unreachable. A talker on threadx-linux now reserves 11,069 words where its
executor needs 2,917.

It is not a memory regression — the rung MOVES bytes between `.bss` and the
byte pool, which gives back the same number — but it makes `mem-report`'s
`.bss` figure describe a reservation nothing uses, which is what phase-392 W6
moved the backing to `.bss` to avoid.

## The fix that was written and measured, and not shipped

`cmd.env("NROS_EXECUTOR_BACKING_U64S", "0")` on the nested probe spawn, in
`nros-sizes-build`. Zero is the documented opt-out: no static, no
`nros_executor_backing_static` cfg, and so no const assertion. Nothing the
probe measures changes — the static is a `.bss` reservation `backing::take`
hands out, not a term in `ExecutorSizing`, and the header's
`NROS_EXECUTOR_SIZE` is the executor VALUE plus its carved storage.

MEASURED to work: with it, the C and C++ halves (`_cargo-build_nros_c`,
`_cargo-build_nros_cpp`) build under a claim the probe had been rejecting.

It was left out of 1388's fix deliberately: with the claim at 11069 the build
is green WITHOUT it, verified by reverting it and rebuilding, so shipping it
there would have claimed a necessity it does not have.

## Acceptance

Not "the probe stops failing" — it does not fail today. The acceptance is that
a board can state a number BELOW the unnarrowed default and still build:

1. apply the probe fix;
2. address `nros_ws_runtime` taking the unnarrowed default, or accept that it
   pins the floor;
3. lower `nros-board-threadx-linux`'s claim toward its per-image requirement
   and show `just threadx_linux build-examples` green.

Step 2 is the real question and may be the larger half: a per-cargo-root crate
has no single image's knobs by construction, so either it must not carry the
static, or the claim must be per-image rather than per-board.


## WONTFIX 2026-09-21 — the pin is necessary but not sufficient, and buys nothing today

Decided by the maintainer, who closed PR #1108 (the fix described below, written
and measured to work) unmerged. Recorded here with the reasoning so this reads
as a decision rather than as work nobody got to.

### It changes no behaviour today

`nros-board-threadx-linux` states **11069**, which IS the unnarrowed
`ExecutorSizing::DEFAULT` — the number the probe judges against. So the probe's
verdict currently agrees with the image's requirement and nothing fails. Issue
1388, the breakage this mechanism caused, is FIXED and merged; the claim was
raised there for an independent reason.

What the pin buys is an OPTION, not a repair: the ability for a board to state a
number below the unnarrowed default.

### And the option is not actually available, because the pin is only half

Measured while resolving this issue (one serial `just threadx_linux
build-examples`, 18 `nros-node` compilations instrumented):

| units | knobs | board rung | what they are |
| --- | --- | --- | --- |
| 5 | narrowed | `Some(11069)` | the standalone `examples/threadx-linux/rust/*` leaves; derived defaults 2917 / 3626 / 4494 |
| 11 | UNNARROWED | `None` | `nros_c-static`/`nros_cpp-static` in three cmake roots, two cargo-built entries, four size probes |
| 2 | UNNARROWED | `Some(11069)` | the `nros_ws_runtime` umbrella of `workspaces/mixed`, and the probe spawned beneath it |

Only those last two can fail `stated >= default`. The probe is ONE of them. The
other is the **umbrella**, which is written per cmake configure and compiled per
CARGO ROOT — one serves every entry in the workspace, so it can carry no single
image's narrowing **by construction**. Pinning the probe therefore does not let
a board lower its claim: the umbrella still demands the maximum.

Making the umbrella decline the static was considered and rejected on its own
merits: the same cargo root compiles the board crate, whose
`forward_executor_backing_words` subtracts `8 *` the same rung from the ThreadX
byte pool. Declining on one side without suppressing the subtraction on the
other loses the bytes from BOTH — a worse outcome than the over-statement.

So a real fix is not the one-line pin. It is either "the umbrella does not carry
this static, and the pool subtraction follows it", or "the claim becomes
per-image rather than per-board". Both are design changes, and neither is
justified by what the over-statement currently costs.

### What it costs to leave

A threadx-linux talker reserves 11,069 words for an executor that needs 2,917.

That is **not memory**. This rung MOVES bytes between `.bss` and the byte pool,
which gives back the same count, so the image's total is unchanged. What it
costs is ACCURACY: `mem-report` reads symbols, and the `.bss` figure on this
board describes a reservation about 3.8x larger than anything uses. Making that
figure honest is the reason phase-392 W6 moved the backing off the heap, so this
is a real cost to the campaign's own instrument — just not one that breaks a
build or a byte budget.

### What would make this worth reopening

* A board that needs to state a number below the unnarrowed default for a reason
  the maximum cannot satisfy — a pool too small to carry it, which is the
  constraint the per-role measurement in issue 1388 was originally reaching for.
* `mem-report`'s `.bss` figure being used to make a sizing decision on an RTOS
  board, where a 3.8x over-statement would mislead rather than merely inflate.
* The umbrella question being settled for another reason (issue 1402 touches the
  same seam — the knobs the umbrella does and does not receive).

### What survives this decision

The measurement above, and the correction it forced: `nros_c-static` /
`nros_cpp-static` receive NO board facts (`nros_board_facts_env` is called only
for the umbrella and rv-virt-threadx), so the C/C++ lanes never judged the claim
at all. This issue's original text implied they did.

The pin itself is written and measured — closed PR #1108 — if a future reason
makes it worth taking.
