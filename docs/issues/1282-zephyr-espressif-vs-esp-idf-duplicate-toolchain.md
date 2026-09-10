---
id: 1282
title: "`hal_espressif` has no measured consumer and is retained anyway — is our
  separate ESP-IDF provisioning a duplicate of Zephyr's own espressif support?"
status: open
type: tech-debt
area: build, esp32
severity: low
found: 2026-09-11
related: [issue-1275, issue-0500]
---

## What this is

phase-447 F1 brought the Zephyr module set under `nros-sdk-index.toml` and
narrowed the west allowlist, dropping `hal_nxp`, `hal_stm32` and `hal_nordic`.
It **kept `hal_espressif`** — 275 MB of every fresh `west update` — deliberately,
and this issue is the reason it kept it and the question that decides whether it
stays.

Issue 1275 asked three questions. F1 measured the first two:

**(1) Does anything build an esp32 image THROUGH Zephyr?** No. Every esp32
coordinate is cargo/esp-hal bare-metal riscv32 or ESP-IDF:

| coordinate | build system |
| --- | --- |
| `workspace-rust-esp32` (`examples/fixtures.toml`) | cargo, `riscv32imc-unknown-none-elf` |
| `esp32-c3-baremetal-{talker,listener,logging-smoke}` | cargo (esp-hal) |
| `tests/esp-idf-smoke`, `integrations/nano-ros` | `idf.py` |

No `platform = "esp32"` row carries a `board =` at all — `board` is required only
for `platform = "zephyr"`. `just/esp32.just` drives cargo + espflash + the
Espressif QEMU fork; `just/esp_idf.just` drives `idf.py`. Neither mentions `west`.

**(2) Does `hal_espressif` have any other consumer?** No. It appears in exactly
four places, none of them a build: the two west allowlists and two prose docs.
Zero `prj.conf`, overlay, board crate, CI job, or `west build -b <espressif>`
anywhere. The single Zephyr-side esp branch —
`zephyr/cmake/nros_cargo_build.cmake:78`, `elseif(CONFIG_SOC_SERIES_ESP32C3)` —
is unreachable, because nothing configures a Zephyr build with an Espressif
board. And `scripts/zephyr/setup.sh` installs only `x86_64-zephyr-elf` and
`arm-zephyr-eabi` (plus `aarch64-zephyr-elf` behind a flag): **no
`xtensa-espressif_*` toolchain**, so a Zephyr esp32 image could not link today
even if a board named one.

By 1275's own step 2 that would put it out of the allowlist with the other
three. It stays, because RFC-0099 D10 and phase-447 F1 both say the third
question is not one to settle while deleting a manifest line — and deleting it
settles it, in the direction of "we do not do Zephyr esp32".

## The question that is actually open

**Zephyr ships espressif support of its own. Is our separate ESP-IDF
provisioning a duplicate?**

If a Zephyr-hosted esp32 build would serve the same coordinates, we carry two
toolchains, two provisioning paths and two sets of build rules for one target —
the "two producers for one prefix" shape of issue 0500, one level up, at the
toolchain rather than the tool. Today we provision: `[tool.esp32-qemu]` (a
source-built Espressif QEMU fork), `[tool.espflash]`, an `esp-idf-workspace`
cloned by `scripts/esp_idf/setup.sh` (ESP-IDF is not even in the index), AND a
Zephyr SDK — with `hal_espressif` sitting in the Zephyr workspace using none of
it.

There are real costs on both sides. ESP-IDF is what Espressif supports and what
the esp32 examples are written against; the bare-metal rows are esp-hal, which
is neither. So this wants a measurement and probably an RFC, not a decision
taken in passing.

## What would close this

Either answer, recorded:

* **Zephyr's espressif support does not serve our coordinates** → set
  `lines = []` on `[zephyr_module.hal_espressif]` in `nros-sdk-index.toml`,
  remove it from both west allowlists (`check-zephyr-module-allowlist` will
  demand the pair move together), and delete the dead
  `CONFIG_SOC_SERIES_ESP32C3` branch in `zephyr/cmake/nros_cargo_build.cmake`.
  That reclaims the last 275 MB of 1275's 2.5 GB.
* **It could serve them** → an RFC on which path ships, with the toolchain
  duplication priced.

Until then the index says plainly that we pay 275 MB for a module nothing
consumes, which is the honest state rather than a hidden one.

## Where this was measured

phase-447 F1, 2026-09-11, on the branch that closed 1275. Sizes from `du -sh` on
a populated `zephyr-workspace`; the consumer sweep covered `examples/`,
`cmake/`, `packages/`, `just/`, `.github/workflows/`, `scripts/`, `zephyr/` and
every `prj.conf`/`*.overlay`/`Kconfig*` in the tree, excluding `third-party/`
and `zephyr-workspace*`.
