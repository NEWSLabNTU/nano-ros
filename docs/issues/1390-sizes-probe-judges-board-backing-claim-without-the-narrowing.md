---
id: 1390
title: "The sizes probe inherits a board's executor-backing CLAIM but not the
  image's knob narrowing, so it judges the claim against a default belonging to
  no image — which forces every board claim up to the unnarrowed maximum"
status: open
type: bug
area: [build, core]
severity: medium
found: 2026-09-20
related: [1388, 1145, 1171, 1284, 0460]
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
