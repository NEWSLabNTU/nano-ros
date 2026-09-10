---
id: 1275
title: "The west manifest's HAL allowlist pulls 2.5 GB of vendor HALs no board
  we build needs — and whether Zephyr's espressif support makes our ESP-IDF
  path a duplicate is an open question"
status: open
type: tech-debt
area: build, testing
severity: medium
found: 2026-09-10
related: [issue-1266, issue-1267, issue-1274]
---

## What this is

`west.yml` narrows Zephyr's ~90 modules to 11 with a `name-allowlist`, which is
the right shape. The question is which 11. Measured against an existing
workspace on this host:

| module | size | needed by |
| --- | --- | --- |
| `hal_nxp` | 1.3 G | nothing we build |
| `hal_stm32` | 764 M | nothing we build |
| `hal_espressif` | 275 M | see below |
| `hal_nordic` | 224 M | nothing we build |
| `cmsis` | 6.6 M | mps2_an385, qemu_cortex_a53 |

Every Zephyr board in `examples/fixtures.toml`:

```
72  board = "native_sim/native/64"
 3  board = "mps2_an385"
 1  board = "qemu_cortex_a53/qemu_cortex_a53/smp"
```

`native_sim` needs no vendor HAL at all; the other two need `cmsis`. So roughly
**2.5 GB of every `west update` is for silicon nothing in this repo targets** —
paid by CI on every fresh workspace and by every contributor on their first
setup.

The manifest says as much itself:

```yaml
# HALs (add more as needed for your boards)
- hal_stm32
- hal_nordic
- hal_espressif
- hal_nxp
```

Added ahead of need, and nothing since has needed them.

## Do not just delete the four

Three of them (`hal_nxp`, `hal_stm32`, `hal_nordic`) have no plausible consumer
here and the change is a four-line diff. But prove it rather than assume it: a
Zephyr `prj.conf` or board fragment can pull a module without any fixture naming
the board. Grep the conf tree and build one native_sim and one mps2_an385 leaf
against the narrowed manifest before closing.

Removing a module from the allowlist also does not reclaim an EXISTING
workspace — `west update` does not prune. Say so in the change, or a reader
measures no improvement and reverts it.

## The espressif half is a real question, not a cleanup

DECIDED (2026-09-10): research this separately rather than folding it into the
allowlist change.

esp32 IS a nano-ros platform — `platform = "esp32"` rows exist in
`examples/fixtures.toml`, and `just/esp_idf.just` provisions the ESP-IDF
toolchain for `esp32-c3-baremetal`. But that path builds through ESP-IDF, not
through Zephyr, so `hal_espressif` in the Zephyr workspace is very likely unused
by it.

The larger question that raises: **Zephyr ships an espressif toolchain of its
own, so is our separate ESP-IDF provisioning a duplicate?** If a Zephyr-hosted
esp32 build would serve the same coordinates, we are carrying two toolchains,
two provisioning paths and two sets of build rules for one target — which is the
"two producers for one prefix" shape (issue 0500) one level up, at the toolchain
rather than the tool.

What the research has to answer, in this order:

1. Does anything in the tree build an esp32 image THROUGH Zephyr today, or is
   every esp32 coordinate ESP-IDF? (`examples/fixtures.toml`, the esp32 leaves,
   `just/esp32.just` vs `just/esp_idf.just`.)
2. If it is all ESP-IDF: does `hal_espressif` have any other consumer? If not,
   it leaves the allowlist with the other three.
3. Separately, and NOT a prerequisite for (2): could the ESP-IDF path be served
   by Zephyr's espressif support instead? That is a platform-strategy question
   with real costs on both sides — ESP-IDF is what Espressif supports and what
   the esp32 examples are written against — so it wants a measurement and an RFC,
   not a decision taken while deleting a manifest line.

Answering (1) and (2) is enough to act on the allowlist. (3) can stay open.

## Where this was measured

A contained self-hosted runner bootstrap, 2026-09-10. `west update` spent the
bulk of its wall time on `hal_espressif`, `hal_nordic`, `hal_nxp` and
`hal_stm32`, in that order, before reaching `mbedtls`. It compounds with issue
1266: that 2.5 GB is fetched in the middle of a sequence that reports no
progress.
