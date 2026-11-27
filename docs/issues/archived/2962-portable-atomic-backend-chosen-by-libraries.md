---
id: 2962
title: "No Rust image builds for rpi-pico-w-zephyr: four library crates chose portable-atomic's backend, and it conflicts with the zephyr crate's"
status: resolved
type: bug
severity: high
area: [build, zephyr, boards]
related: [2206, 2772]
found: 2026-11-25
resolved: 2026-11-28
resolved_in: "3d9cea2570"
---

## What happens

Every Rust project for `rpi-pico-w-zephyr` (thumbv6m-none-eabi, Cortex-M0+,
no atomic CAS) fails to compile. This covers all four Rust templates: talker,
listener, server and action-server. Reproduced on main `e5dcabd3ca`:

```text
nros new x --board rpi-pico-w-zephyr --lang rust --rmw zenoh --template talker && cd x && nros build

error: you may not enable `critical-section` feature and `portable_atomic_unsafe_assume_{single_core,privileged}`
       cfg (`unsafe-assume-{single-core,privileged}` feature) at the same time
error: could not compile `portable-atomic` (lib) due to 1 previous error
```

## Cause

On a target with no CAS, portable-atomic needs a backend. It refuses to be
built with both of these at once:

- the `critical-section` feature, where someone supplies the impl;
- `unsafe-assume-single-core`, which disables interrupts.

Four library crates chose `unsafe-assume-single-core` with a target-cfg row,
`cfg(all(target_os = "none", not(target_has_atomic = "ptr")))`:

- `nros-log`
- `zpico-alloc`
- `nros-cpp`
- `nros-smoltcp`

A Zephyr Rust image links upstream's `zephyr` crate, which enables
`portable-atomic` with `["fallback", "critical-section"]`. Feature unification
puts both halves in one graph. The cfg cannot tell Zephyr from bare metal,
because both are `target_os = "none"`. Phase-337 W2.b fixed the thumbv7m case
of this conflict by asking about CAS directly. thumbv6m really has no CAS, so
it kept the row, and no lane builds a Rust image for it.

The choice was never a library's to make. Which backend is right is a fact
about the system, and portable-atomic's own documentation asks libraries to
leave it to the end user.

The C and C++ rows on this board got as far as the build only because no
`zephyr` crate is in their graph. On main they stop at issue 2772's
queryable floor (`NROS_ZENOH_MAX_QUERYABLES=1 is below this image's DERIVED
floor of 6`), which is a separate defect.

## Fix (the class)

**No library crate chooses the backend. The image root does:**

- A Zephyr Rust image: upstream's `zephyr` crate (`critical-section`), as
  before.
- A C/C++ image: `nros-c`'s `critical-section` capability now also forwards
  `portable-atomic/critical-section`. nros-c is that image's cargo root, and
  its `PlatformCs` is the impl. On a target with CAS, portable-atomic uses
  native atomics and never calls it.
- A bare-metal board: its board crate (`packages/boards/`), or the target's
  rustflags. The root `.cargo/config.toml` already sets the single-core cfg
  for `riscv32imc-unknown-none-elf`, ESP32-C3. That is the one place a C
  image on that target would meet nros-c's `critical-section`. The ESP32
  boards are dormant (issue 1525), so it is noted here and not changed.

**The size probe.** `nros-c`'s build script measures the `nros` rlib in a
nested `cargo build -p nros`. That nested build is a library graph alone, so
on thumbv6m it had no backend any more ("dependents require atomic CAS but
not available"). `nros-sizes-build` now gives that nested build the no-impl
single-core cfg on a target without CAS. It asks the build script's own
target facts: `CARGO_CFG_TARGET_HAS_ATOMIC`, which cargo leaves unset on
thumbv6m. Only the layout is read from the rlib, and a type's size does not
depend on how its atomics are implemented.

**Gate:** `just check portable-atomic-backend` refuses a tracked Cargo.toml
outside the owners (nros-c, `packages/boards/`, `examples/`) that enables
`critical-section`, `unsafe-assume-single-core` or `unsafe-assume-privileged`
on portable-atomic. It covers a dependency feature in any table, a renamed
dependency, and a `[features]` forward. On main it finds the four crates
above.

## Verified (host 118, Zephyr 3.7, SDK 0.16.8)

`nros new <x> --board rpi-pico-w-zephyr --rmw zenoh --template talker`, then
`nros build`, with the fix. Each image raised its queryable knob
(`[image.zephyr.knobs.zenoh] max_queryables = 8`) to get past issue 2772:

| Language | Result | RAM | FLASH |
| --- | --- | --- | --- |
| Rust | links | 197,160 B (72.9 %) | 405,600 B |
| C++ | links | 181,776 B (67.2 %) | 413,608 B |
| C | links | 182,568 B (67.5 %) | 419,176 B |

On main, Rust fails at portable-atomic, and C and C++ fail at the issue 2772
floor.

**Other thumbv6m boards:** none. `rpi-pico-w-zephyr` is the only board whose
`rust_target` is CAS-less (`git grep -n 'thumbv6m\|riscv32imc' --
'packages/boards/*/nros-board.toml'`).

**A trap met while verifying:** a failed metadata probe leaves
`src/<pkg>/metadata/<pkg>.json.unprobeable` in the project's SOURCE tree. A
later `nros build`, even after `rm -rf build`, reports that cached failure
without probing again. Delete the marker before believing a second "probe
FAILED".

## Resolved (2026-11-28)

The fix landed as 3d9cea2570 (PR #4847): no library crate chooses the
backend, and nros-c and the size probe are covered as above. Its gate is on
main, `just check portable-atomic-backend`, with a self-test; on main it reads
"no library crate chooses portable-atomic's backend".

**Acceptance, all four Rust templates on the Pico.** Host 133, 2026-11-27,
ran issue 2772's sweep: `nros new <x> --board rpi-pico-w-zephyr --rmw zenoh
--template <t>` then `nros build`, Zephyr 3.7. The base was PR #4843's head
then, with PR #4847's 12 files applied. Every Rust row links:

| Template | Rust RAM (of 270,336 B) |
| --- | --- |
| talker | 194,792 B (72.06 %) |
| listener | 193,064 B (71.42 %) |
| server | 194,704 B (72.02 %) |
| action-server | 208,968 B (77.30 %) |

The Nucleo-F767ZI Rust rows, a board with CAS, linked in the same run.

The base carried #4843 because the Pico's queryable floor is issue 2772's
defect, not this one. That fix is #4843, and issue 2772 records the full
24-row table there. On main without #4843, a Pico image stops at that floor,
not at portable-atomic.

