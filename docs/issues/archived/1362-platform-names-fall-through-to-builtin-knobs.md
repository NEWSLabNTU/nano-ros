---
id: 1362
title: "TWO platform names boards declare are answered by no
  `nros-platform.toml` and take the BUILTIN knob defaults — the other three
  were fixture coordinate labels the gate mistook for platform names"
status: resolved
type: bug
area: build, boards
severity: medium
found: 2026-09-13
resolved_in: "fix(#1362, phase-468 W1): a platform with no descriptor is an ERROR"
related: [1145, 0196, 1382, 1486, 1494, phase-448, phase-468]
---

## What this is

A platform descriptor's `names` is the list of names that platform answers to;
the directory it lives in is only where the file sits. A name no descriptor
lists is an `UnknownPlatform`, and `or_builtin_rungs`
(`nros-platform-config/src/platform_config.rs`) turns that into a
`cargo:warning` plus the BUILTIN knob defaults — three times per build, once
each for the executor, params and memory rungs.

That fall-through is deliberate and the comment says why: it keeps a TYPO in
`NROS_PLATFORM_NAME` visible instead of silently selecting builtins. Right for
a typo. Wrong for a platform that simply never declared its own name, because
the builtins it lands on are not that platform's numbers.

## Measured 2026-09-13

`NROS_PLATFORM_NAME=<name> cargo check -p nros-node`, with `build.rs` touched
so the script re-runs:

| name | fall-through warnings |
| --- | ---: |
| `esp32` | 3 |
| `baremetal` | 3 |
| `nuttx-riscv` | 3 |
| `freertos-posix` | (same shape) |
| `zephyr-cortex-m` | (same shape) |

All five appear as `platform = "…"` rows in `examples/fixtures.toml`, so each
is a platform the tree actually builds.

## Why this was worth its own issue rather than a five-line fix

Issue 1145 hit the SAME class one name over: `threadx-linux` fell through, took
builtin executor knobs below `EXECUTOR_BACKING_DEFAULT_U64S`, and the backing
guard then refused the reservation — so the platform stopped COMPILING, in a
nightly, with an error naming neither the descriptor nor the name. That one is
fixed (the threadx descriptor answers to its two variants now).

Fixing the other five is **not** the same edit. Adding a name to a descriptor
swaps builtin knob values for that descriptor's, which is a behaviour change per
platform:

* `nuttx-riscv`, `freertos-posix`, `zephyr-cortex-m` each have a plausible
  parent descriptor, and adding the name is probably right — but "probably" is
  the word that turns one fixed regression into four new ones. Each wants its
  knobs compared before and after, on that platform's own image.
* `esp32` and `baremetal` have **no platform package at all**. For them it is
  not a missing name, it is a decision: do they get a descriptor, or is
  falling through to builtins the correct answer for a platform with no RTOS
  to describe?

## Gated meanwhile

`check-platform-name-answered` (fast line, buildless) asserts that every
`platform = "…"` in `examples/fixtures.toml` is answered by some descriptor's
`names`. These five are a RATCHET — `BASELINE_UNANSWERED`, which may only
shrink — so a SIXTH cannot appear unnoticed while these wait.

The gate is mutation-tested: reverting the threadx fix makes it report both
threadx names, and a baselined name that starts resolving is reported STALE
rather than silently kept.

## Fix

Per name, in this order: compare the knobs that platform gets today (builtins)
against the ones it would get from the candidate descriptor, on a built image;
then either add the name and drop it from the baseline, or record why
falling through is correct for it. `esp32` and `baremetal` need the decision
above first.


## CORRECTED 2026-09-18 — three of the five were never fall-throughs

The measurement above is circular, and it took three names with it. It sets
`NROS_PLATFORM_NAME=<name>` **by hand** and observes `or_builtin_rungs` warn.
That proves the lookup answers an unknown name. It does not prove any build
ever asks it one, and the sentence that bridged the gap — *"All five appear as
`platform = "…"` rows in `examples/fixtures.toml`, so each is a platform the
tree actually builds"* — is the unchecked step.

A fixture row's `platform` is a **coordinate label**. It names the lane cell
and feeds `build_subdir`; nothing looks a descriptor up by it. The name that
reaches the lookup is emitted by `nros ws board-facts` from
`descriptor.platform` (`nros-cli-core/src/cmd/board_facts.rs:233`) — the
`platform = "…"` a **board** declares. The rows say so themselves:

```toml
platform = "freertos-posix"                                   # coordinate label
cmake_defs = { NANO_ROS_PLATFORM = "freertos",
               NANO_ROS_BOARD = "freertos-posix" }             # what is passed
```

Measured through the live path, not read off the files:

```
$ nros ws board-facts examples/workspaces/c/src/demo_bringup --board freertos-posix
NROS_PLATFORM_NAME=freertos
$ nros ws board-facts examples/workspaces/realtime-cpp/src/demo_bringup --board rv-virt-nuttx
NROS_PLATFORM_NAME=nuttx
$ nros ws board-facts examples/workspaces/features/src/demo_bringup --board zephyr
NROS_PLATFORM_NAME=zephyr
```

All three are answered descriptors. `freertos-posix`, `nuttx-riscv` and
`zephyr-cortex-m` never fell through, so three fifths of this issue was work
that did not exist — and the "compare the knobs before and after" protocol
would have compared nothing, because `freertos` and `nuttx` declare **no
`[knobs.*]` at all**.

### What is actually left

Two, and the spelling matters:

| name | declared by | why it is a decision |
| --- | --- | --- |
| `esp32` | `nros-board-esp32-qemu/nros-board.toml` | has an RTOS (ESP-IDF's FreeRTOS) and no descriptor; probably wants one |
| `bare-metal` | `nros-board-mps2-an385/nros-board.toml` | no RTOS to describe; falling through may be the correct answer |

The old baseline spelled the second one `baremetal`, which is the fixtures
label. No board declares that string, so even the one real entry named
something this population never produces.

Both still need their knobs compared on a built image before either is claimed.

### The gate now reads the right population

`check-platform-name-answered` walked `examples/fixtures.toml`. It now walks
`packages/boards/*/nros-board.toml` and reads each `[[board]]` element's
`platform` — an array of tables, so a single file may declare several boards
with different platforms, which a top-level key read would have missed.

It keeps the catch it was built for: `nros-board-threadx-linux` declares
`platform = "threadx-linux"`, so that name is in this population. Mutation
tested — reverting the threadx descriptor to `names = ["threadx"]` reports both
threadx names, each with the board file that declares it.

`NOT_A_DESCRIPTOR_NAME` is empty now. It held `{"native", "linux"}` to excuse
fixture rows spelling a ROLE in their coordinate label; no board declares
either (`packages/boards/linux` declares `platform = "posix"`, correctly), and
a board that did would be making the claim CLAUDE.md's Naming section forbids —
which this gate should report, not excuse.

Same shape as the rest of this campaign: the gate was authored, registered,
mutation-tested, and watching the wrong names. It caught issue 1145 only
because that board's declared platform and its fixtures label happen to be the
same string.

## RESOLVED 2026-09-25 — phase-468 W1

Both remaining names are answered, and the fall-through they were falling
through is gone.

* `bare-metal` was never unanswered. `config/bare-metal/nros-platform.toml` has
  answered it since phase-349 W1; the gate read ONE of the loader's two search
  roots and so reported it, and the repair at the time was to baseline it rather
  than widen the reach — issue 1486.
* `esp32` was the one real fall-through, and the decision this issue said was
  needed went the way its own table did not expect. It reads "has an RTOS
  (ESP-IDF's FreeRTOS) and no descriptor; probably wants one". That conflates
  the two esp32 paths phase-468 W2 warns about: the board declaring
  `platform = "esp32"` is `nros-board-esp32-qemu`, the **bare-metal ESP32-C3
  QEMU** board (esp-hal, riscv32imc), not the ESP-IDF port. It has no RTOS, and
  `config/bare-metal` was already its descriptor on every other road —
  `PlatformKind::Esp32::platform_feature()` returns `platform-bare-metal`, the
  board takes `nros-rmw-zenoh/platform-bare-metal` so `nros-zpico-build`
  resolves that file for its vendored zenoh-pico C build, and
  `[arch.riscv32imc]` in it was written for the ESP32-C3. `esp32` joined that
  file's `names`.
* The "compare the knobs on a built image before claiming it" protocol was
  followed and the comparison is empty by construction: `config/bare-metal`
  declares no `[knobs.*]`, so the rungs esp32 resolves through it are
  byte-identical to the builtins it was falling through to. What changed is the
  diagnosis, not the image.

And the shape that made this an issue rather than an edit is retired.
`or_builtin_rungs` is `require_rungs`: an `UnknownPlatform` panics with a
message naming the platform, every root searched, the names the tree answers to
and the remedy. `check-platform-name-answered` has no `BASELINE_UNANSWERED` and
no ratchet at all — there is nothing left to baseline into, so a sixth name
cannot wait, it can only be answered. Measured: all 8 board-declared names build
with 0 `cargo:warning` fall-through lines; an invented name exits 101.
