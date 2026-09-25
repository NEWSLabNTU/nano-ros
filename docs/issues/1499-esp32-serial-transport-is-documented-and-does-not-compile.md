---
id: 1499
title: "ESP32 serial transport is documented as working, has no backend, and does not compile"
status: open
area: boards
severity: medium
phases: [468]
rfcs: [0064]
related: [0652, 0612, 0667, 1493]
---

# ESP32 serial transport is documented as working, has no backend, and does not compile

`book/src/user-guide/serial-transport.md` presents ESP32 serial as a supported
configuration, with a feature line to copy:

```toml
nros-board-esp32-qemu = { version = "*", default-features = false, features = ["serial"] }
```

That configuration fails to compile. Measured on `main`:

```
$ cd packages/boards/nros-board-esp32-qemu
$ cargo build --no-default-features --features serial --target riscv32imc-unknown-none-elf
error[E0609]: no field `mac_addr` on type `&config::Config`
  --> src/node.rs:27:24
error[E0609]: no field `ip` on type `&config::Config`
  --> src/node.rs:27:53
   = note: available fields are: `baudrate`, `zenoh_locator`, `domain_id`
```

`node.rs:27` reads `config.mac_addr` and `config.ip` unconditionally, and both
are `#[cfg(feature = "ethernet")]` fields on `Config`. So the serial-only build
is broken at the first function that touches the config.

## There is also no serial backend, which the compile error hides

Even with that cfg repaired the image cannot link, and the reason is one level
down. The board's feature is:

```toml
# Serial transport via zenoh-pico built-in serial (no additional deps)
serial = []
```

It pulls nothing — no `zpico-serial`, no UART driver — and all it produces is a
`serial_default()` preset: a baud rate and a `serial/UART_0#baudrate=115200`
locator string. Nothing provides `_z_open_serial_from_dev` /
`_z_read_serial_internal` / `_z_send_serial_internal`.

The comment's premise is false for this board. `zpico-serial`'s own module doc
says what it is for:

> Provides the zenoh-pico serial platform symbols … **for bare-metal targets
> where zenoh-pico has no built-in serial backend.**

ESP32-QEMU **is** such a target — phase-468 W1 established that `esp32` is
bare-metal (esp-hal, riscv32imc), answered by `config/bare-metal/nros-platform.toml`.
The vendored zenoh-pico tree carries exactly one serial source,
`src/system/common/serial.c` (the framing half), and the low-level open/read/write
live in a per-platform `network.c`; there is no bare-metal one. The three crates
that actually depend on `zpico-serial` are `nros-board-mps2-an385`,
`cmsdk-uart` and `stm32f4-usart` — not esp32.

So the book's own table already contradicted itself, one page apart:

| line | claim |
| --- | --- |
| `serial-transport.md:41` | ESP32-QEMU (esp-hal, no IDF) — **`zpico-serial` path, same as bare-metal** |
| `serial-transport.md:293` | ESP32 uses zenoh-pico's built-in serial implementation, **needs no `zpico-serial` dependency** |

Line 41 is right about the mechanism and wrong that it is wired; line 293 is
right that no dependency is declared and wrong about why.

## Why nobody noticed

**Nothing builds it.** No `examples/fixtures.toml` row, no example leaf and no
test names esp32 with `serial` — the search returns zero. This is the class
CLAUDE.md records for `required-features` targets no recipe enables: cargo
skips them silently, so the feature reads as coverage while never being
compiled. When that family was finally laned (issues 0652 / 0612 / 0667), four
targets were broken and one capability was entirely non-functional. This is a
fifth instance, reached from the documentation side rather than the manifest
side.

The ESP-IDF qualifier in that paragraph was corrected during phase-468 W2 —
`check-zenoh-source-manifest.py` now records that nothing selects zenoh-pico's
`src/system/espidf/` tree — but correcting the qualifier left the surrounding
claim standing, and the claim was the part that was wrong.

## The fix, in two independent halves

1. **The book stops offering it** (done — the page now says the path is not
   wired, and points here). A documented configuration that fails to compile is
   worse than a documented gap.
2. **Either wire it or delete the feature.** Wiring it means the `node.rs` cfg
   repair, a `zpico-serial` dependency behind the feature, and a UART driver for
   the ESP32-C3 peripheral (`packages/drivers/serial/` holds the two existing
   ones, `cmsdk-uart` and `stm32f4-usart`). Deleting it means removing
   `serial = []` and its `Config` arms. **Whichever is chosen, it needs a
   fixture row** — that is the only thing that stops this recurring, and its
   absence is why this issue exists.

## Acceptance

- [ ] The serial-only board either compiles and links in a built image, or the
      feature is gone.
- [ ] If kept: an `examples/fixtures.toml` row builds it, so the target is
      reachable rather than nominal.
- [ ] `git grep -n 'esp32.*serial' -- examples/ book/` agrees with whichever
      was chosen.
