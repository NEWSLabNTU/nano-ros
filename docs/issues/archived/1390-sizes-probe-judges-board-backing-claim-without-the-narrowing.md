---
id: 1390
title: "The sizes probe inherits a board's executor-backing CLAIM but not the
  image's knob narrowing, so it judges the claim against a default belonging to
  no image — which forces every board claim up to the unnarrowed maximum"
status: resolved
resolved: 2026-09-21
type: bug
area: [build, core]
severity: medium
found: 2026-09-20
related: [1388, 1197, 1145, 1171, 1284, 0528, 0460]
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

## RESOLVED 2026-09-21 — the probe fix ships; the acceptance's step 3 does NOT, and the measurement says why

The probe is fixed. The board's claim stays at 11069, and that is now a
MEASURED decision rather than the leftover of issue 1388.

### What was measured

`nros-node`'s build script was instrumented to append, for every unit it
compiles, the knobs it resolved and the answer the RFC-0049 ladder gave for
`NROS_EXECUTOR_BACKING_U64S`, plus the `OUT_DIR` that identifies the cargo root.
One serial `just threadx_linux build-examples` (2026-09-20, rc=0) then compiles
`nros-node` **18 times**. Sorted by what the rung reaches:

| units | knobs | board rung | what they are |
| --- | --- | --- | --- |
| 5 | narrowed (`cbs=1..2`, arena 14424–26800) | `Some(11069)` | the six standalone `examples/threadx-linux/rust/*` leaves, narrowed by `nros sync` — derived defaults 2917 / 3626 / 4494 |
| 11 | UNNARROWED (`cbs=4`, `sc=8`, arena 74240) | `None` | `nros_c-static` / `nros_cpp-static` in the three cmake workspace roots, the cargo-built `workspaces/rust` and `workspaces/realtime-rust` entries, and four size probes |
| **2** | **UNNARROWED** | **`Some(11069)`** | **the `nros_ws_runtime` umbrella of `workspaces/mixed`, and the size probe spawned beneath it** |

Only the last row can fail the assertion, because only there do a board's
statement and a default belonging to no image meet. The 11 `None` units size the
static at their own default, so `stated >= default` there compares a number with
itself; the 5 narrowed units have defaults well under the claim.

So the population that forces the claim up was exactly **two units**, and this
issue removes one of them.

### The probe fix, and a positive control for it

`packages/tooling/nros-sizes-build/src/lib.rs`: the env scrubbing is now
`isolate_probe_env()`, and it PINS `NROS_EXECUTOR_BACKING_U64S=0` on the nested
command. Pinning rather than removing is the point — removing the variable would
leave the board rung on the ladder, which is the whole defect.

Its necessity is not a reading. Running the nested command by hand
(`NROS_PLATFORM_NAME=threadx-linux NROS_BOARD=threadx-linux NROS_BOARD_TOML=<board>
cargo build -p nros --target x86_64-unknown-linux-gnu --no-default-features
--features alloc,env,rmw-cffi,ros-humble,std`), one fresh target dir, three
cases:

```
A  claim 11069, no pin   rc=0    45 crates compiled
B  claim  4494, no pin   rc=101  error[E0080]: evaluation panicked:
                                 NROS_EXECUTOR_BACKING_U64S is below the default
                                 executor sizing …
C  claim  4494, pin=0    rc=0
```

B is the probe's failure, reproduced outside it. C is the fix.

(A first attempt at this control passed in all three cases and proved nothing:
`BuildRungs::from_build_env()` returns `None` without `NROS_PLATFORM_NAME`, so
the board rung was never read. The board TOML *is* watched — `platform_config`
emits `cargo:rerun-if-changed` on it — but only once something resolved it.)

### Gate coverage, mutation-tested in both directions

Two unit tests in `nros-sizes-build` (root workspace member, so `cargo test
--workspace` / `just ci gate` runs them):

* `the_nested_probe_declines_the_executor_backing_static` — inspects the built
  `Command`'s `get_envs()`: the pin is present and set to `0`, the eight
  cross-build variables are removed, and the target-scoped `RUSTFLAGS` spelling
  is deliberately untouched.
* `the_pinned_backing_knob_is_also_argued_out_of_the_key` — the knob is also
  added to `KNOBS_THAT_CANNOT_CHANGE_A_SIZE`, because `knob_identity()` sweeps
  every `NROS_*` into the probe key (issue 0528) and keying on a value the child
  never sees only splits the shared directory. That exclusion is sound ONLY
  because the pin exists, so the test asserts both halves.

Deleting the pin reds both tests; deleting the table row reds the second:

```
the nested probe must PIN NROS_EXECUTOR_BACKING_U64S=0 (issue 1390). Unpinned,
it takes the board rung off the ladder through the NROS_BOARD_TOML the probe has
to inherit, and `nros-node`'s `stated >= default` assertion then rules on the
board's claim against a default belonging to no image.

NROS_EXECUTOR_BACKING_U64S is argued out of the probe key on the grounds that
the nested command pins it — restore the pin, or remove the exclusion
```

### Step 2, answered: the umbrella pins the floor — and so does vouchability

The acceptance asked for `nros_ws_runtime` to be dealt with or for the floor to
be established. It is a floor, for two reasons that are independent of each
other:

1. **Mechanism.** The umbrella is written per CMAKE CONFIGURE and compiled per
   CARGO ROOT (`cmake/NanoRosRuntimeCrate.cmake`); one of them serves every
   entry in the configure. `nros_board_facts_env()` hands it the board
   descriptor — correctly, it needs the board's other rungs — and there is no
   single image whose narrowing it could take. Making it decline the static
   instead was considered and not done: the same cargo root also compiles the
   board crate, whose `forward_executor_backing_words` subtracts `8 *` the same
   rung from the ThreadX byte pool, so declining the static without also
   suppressing the subtraction loses the bytes from both sides, and any Rust
   `Executor::open` reached from that root would then allocate out of a pool
   that was shortened for a reservation nobody made.

2. **Vouchability, which holds even if the umbrella were fixed.** Issue 1197
   established that nothing outside `nros-node`'s own dependency graph can
   DERIVE this size — it is `arena + repr(Rust) tables`, laid out by the target
   compiler. The only number a host-side lane can measure is therefore the
   unnarrowed `ExecutorSizing::DEFAULT`; a board's narrowed MAXIMUM could be
   established only by building every image for that board. So a claim below the
   default is a claim nothing vouches for, which is precisely the state issue
   1388 was caused by. Lowering this rung would buy about 52 KB of byte pool in
   six leaf images and pay for it by trading a measured claim for an asserted
   one.

The decision is recorded where it will be read: in
`nros-board-threadx-linux/nros-board.toml` beside the number, and in the failure
text of `executor_backing_claims.rs`, so the next person who lowers the claim is
told why it goes back up rather than just that it must.

### What this issue claimed and what remains true

The issue's framing — "the rung's whole purpose is to state what THIS image
reserves" — is not reachable for a rung stated once per BOARD. What the fix buys
is that the probe, which is a measurement artifact rather than a consumer (it
never links, never opens an executor, and the reservation is not a term in
anything it measures), is out of the population that judges a board's claim. The
umbrella is the one remaining floor-setter, and it is a real one.

### What was NOT verified

* **No image was RUN.** This is a compile-and-link result
  (`just threadx_linux build-examples`, rc=0); nothing here executed a ThreadX
  binary or measured a byte pool at runtime.
* **The probe's answer was checked for CHANGE, not re-derived.** The generated
  `NROS_EXECUTOR_SIZE` / `*_OPAQUE_U64S` in every threadx-linux workspace header
  are byte-identical before and after the pin (90408 / 217 / 194 / 194 / 811 /
  666 on the C++ side). That the values are CORRECT is the existing probe's
  claim, not this issue's.
* **Only threadx-linux was measured.** `nros-board-threadx-qemu-riscv64` states
  no backing rung at all (issue 1145's open remainder), and no other board
  carries one, so no other board could be affected — but nothing here built one.
* **The probe-directory sharing argument is reasoned, not measured.** All four
  probe dirs in this lane had distinct keys; a collision between two roots with
  the same target and features and different board rungs was not reproduced.
* **Why the umbrella never receives `NROS_DECLARED_EXECUTOR_MAX_CBS` and friends
  was not investigated.** `_nros_entity_budget_env` exists to deliver exactly
  those, and the measured umbrella env carried only `NROS_DECLARED_NODES=3` and
  `NROS_DECLARED_INFRA_QUERYABLES=none`. If it did fire, the umbrella would take
  the configure's MAX rather than the crate default — still not an image's
  narrowing, so it does not change this decision, but it is a separate thread
  nobody has pulled.
