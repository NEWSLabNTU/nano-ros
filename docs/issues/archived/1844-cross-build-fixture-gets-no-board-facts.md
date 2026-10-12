---
id: 1844
title: "The compile-check cross-build lane asks for no board facts, so both FreeRTOS fixtures build without `FREERTOS_PORT`"
status: resolved
type: bug
severity: high
area: [build, testing, boards]
related: [1827, 1263, phase-484]
found: 2026-10-12
---

## Measured

`just build-test-fixtures lane=tier2` in a clean environment (`env -i`, then
`source ./activate.sh`, so `FREERTOS_PORT` is unset) failed in
`build-compile-check-fixtures`, in both rows of the `cross-build` builder:

```
== cross-build: orch_tiers_freertos (demo_entry @ thumbv7m-none-eabi, profiles: debug,release) ==
error: failed to run custom build command for `zpico-sys v0.5.0 (…/packages/rmw/zenoh/zpico-sys)`
  FREERTOS_PORT not set. FreeRTOS portable layer. Examples: export FREERTOS_PORT=GCC/ARM_CM3 (Cortex-M3) | …
```

and the same for `freertos_firmware`. The matrix lane gate then refused to
run any cell: "17 workspace fixture(s) are missing or stale", "31
compile-check fixture(s) are missing or stale". So tier 2 gave no verdict at
all.

A shell still carrying an older `activate.sh` hides the bug: those exported
`FREERTOS_PORT=GCC/ARM_CM3` repo-wide, and cargo picks it up from the
environment.

## Cause

Since phase-484 W2b, `FREERTOS_PORT` is a board fact and no longer a
repo-wide export. Each road has to deliver it:

| Road | Carrier |
| --- | --- |
| cmake | `nros_resolve_board_facts`; issue 1827 fixed its multi-bringup case |
| single-package cargo leaf | its generated `build/<image>/nros-cargo.toml` |
| `fixtures-build.sh` rows with no settings file | `nros ws board-facts`, asked by the invoker |

`stage_and_cross_build` in `scripts/build/compile-check-fixtures.sh` had none
of these. It ran a bare `cargo build --target thumbv7m-none-eabi` in a staged
workspace-shaped tree. Nothing there has a generated settings file: a
workspace fixture's root, or an entry inside one. Nobody asked the board.

Asked directly, the verb answers for both staged dirs: `GCC/ARM_CM3` and the
board's `FREERTOS_CONFIG_DIR`. So the facts were always available. The lane
just never asked for them.

### The sibling arm

`fixtures-build.sh` does ask, but with `2>/dev/null || true`, so a refusal
there is silent too. Measured over every rust row it builds without a
settings file (55 rows across 6 platforms):

| Rows | Platform | Result |
| --- | --- | --- |
| 2 | FreeRTOS | resolve |
| 3 | ThreadX | resolve |
| 43 | linux (host) | refuse: no board crate |
| 6 | bare-metal | refuse: no board crate, or no port to find |
| 1 | NuttX | refuse: `logging-smoke-nuttx-qemu-arm` depends on `nros-board-nuttx-qemu`, which two descriptors in one `nros-board.toml` both claim |

So no row that needs a port loses it there today. That arm keeps soft
behaviour, now as a stated mode rather than a discarded stderr.

## Fix

Add one shell spelling, `nros_board_facts <dir> required|optional`, in
`scripts/build/board-facts.sh`.

- **`required`:** a refusal is fatal and prints the verb's own message.
- **`optional`:** returns nothing on a refusal.

stdout and stderr stay separate, so a warning printed during a successful
resolve can never become an exported variable.

Callers:

- **`stage_and_cross_build`:** uses `required`. Every row of that builder
  cross-compiles for a board.
- **`fixtures-build.sh`:** its no-settings arm uses `optional`, replacing the
  inline `2>/dev/null || true`.

## Verified

All runs in a clean environment (`env -i`, then `source ./activate.sh`),
with `FREERTOS_PORT` unset:

- **The two cross-build rows build and stamp.** Both
  `orch_tiers_freertos` (debug and release) and `freertos_firmware` wrote
  `.compile-ok` (`NROS_FIXTURE_IDS="orch_tiers_freertos freertos_firmware"`).
  The failing tier-2 run above is the negative control: same lane, no
  facts asked for.
- **The optional arm still builds.** `fixtures-build.sh freertos rust --id
  freertos-logging-smoke-mps2` built through the make leaf, which shows
  `export -f` carries the helper.
- **Both modes behave as stated.**
  - `required` on a dir the verb refuses exits 1 and prints the verb's
    message.
  - `optional` on the same dir exits 0 and prints nothing.

## Not covered

- **No gate:** no check catches a shell lane that spawns cargo for a board
  and asks nothing. `check-board-facts-delivery` covers cmake files only.
  Stating the shell rule textually was not attempted: the cargo spawners in
  `just/` and `scripts/` number in the dozens, and most build for the host.
- **NuttX bin unresolved:** it would need `--board`. It is not a FreeRTOS or
  ThreadX row.
