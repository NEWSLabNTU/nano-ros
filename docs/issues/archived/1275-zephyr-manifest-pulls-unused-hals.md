---
id: 1275
title: "The west manifest's HAL allowlist pulls 2.5 GB of vendor HALs no board
  we build needs — and whether Zephyr's espressif support makes our ESP-IDF
  path a duplicate is an open question"
status: resolved
type: tech-debt
area: build, testing
severity: medium
found: 2026-09-10
related: [issue-1266, issue-1267, issue-1274, issue-1282]
resolved: 2026-09-11
resolved_by: phase-447 F1 (RFC-0099 D10)
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

---

## Resolution — phase-447 F1, 2026-09-11

**Resolved, with one part deliberately not taken.** Read the last section first
if you only want to know what is still paid.

### The module set is index data now

The fix is not the four-line diff this issue warned against. `[zephyr_module.*]`
in `nros-sdk-index.toml` is the SSoT for the Zephyr module set — `why`,
`needed_by`, `approx_mb`, and `lines` (which Zephyr manifest lines carry it) —
and the two west manifests are the derived half:

    a module is in <manifest>'s name-allowlist
      IFF  its `lines` contains that manifest's Zephyr line

`check-zephyr-module-allowlist` asserts that in both directions. The manifests
stay COMMITTED rather than generated, because `west init -m <url>` reads
`west.yml` out of a bare clone before any `nros` exists — the same trade
`check-abi-bindings` makes for committed bindgen output.

`nros setup zephyr --dry-run` now prints the set, largest first, with a total
and the withheld list. That is RFC-0099 D10's actual complaint answered: the
cost is no longer in a file the provisioner never opens.

### What the sweep found — including three boards this issue missed

Every Zephyr board string that is built, run or configured in the tree:

| board | family | HAL needed |
| --- | --- | --- |
| `native_sim/native/64` | host x86-64 | none |
| `mps2_an385` | ARM Cortex-M3 | `cmsis` |
| `qemu_cortex_a53/.../smp` | ARM Cortex-A53 | `cmsis` |
| `qemu_cortex_m3` | TI LM3S6965 | `cmsis` |
| `qemu_cortex_a9` | Xilinx Zynq-7000 | none (in-tree) |
| `fvp_baser_aemv8r/.../smp` | Arm AEMv8-R Fast Model | none |

This issue listed only the first three, because it read `examples/fixtures.toml`
alone. The three it missed are all `cmsis`-or-nothing, so they strengthen the
conclusion rather than change it — but the lesson is that a fixture manifest is
not the board inventory.

The conf-tree sweep the issue asked for came back **empty**: zero
vendor-HAL-pulling CONFIG symbols in any `*.conf`, `*.overlay` or `Kconfig*`
under `examples/`, `cmake/`, `packages/`, `zephyr/` or `tests/`. Not one
`CONFIG_SOC_SERIES_STM32*`, `NRFX*`, `CONFIG_SOC_FAMILY_*` or `ZEPHYR_HAL_*`.
The complete result set is `CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME` (21
files), two QEMU knobs, and one ESP-IDF `sdkconfig.defaults` symbol that is not
Zephyr's Kconfig at all.

### `mcuboot` — why a grep sweep alone would have been wrong

A source grep finds no consumer for `mcuboot` either, and it would have been a
plausible fourth deletion. It is live: **sysbuild** resolves it as a domain, and
a real build says so —
`zephyr-workspace/build-rust-talker-zenoh/sysbuild_modules.txt` names it. The
consumer is a build-system mechanism, not an `#include`. This is exactly the
issue-0876 shape the ask warned about, and it is why the acceptance was a build
rather than a grep.

### `hal_nxp` has a real consumer, and it is not in this tree

`mr_canhubk3/s32k344` (NXP S32K344, Cortex-M7) is the downstream safety-island
board that ~40 in-tree comments cite measurements from — issues 0810/0814/0815,
archived 0734/0961/1227, `zephyr/Kconfig`, `zephyr/CMakeLists.txt`,
`nros-sdk-index.toml`'s own `[tool.zephyr-sdk-1-0-1]` note. There is no board
dir, crate, fixture row or `.conf` for it HERE, so no build in this repo needs
`hal_nxp` — but a downstream build does.

Dropping it silently would have broken that user's `west update` with nothing to
explain why. `[zephyr_module.hal_nxp]` records the board by name and states the
remedy: **a downstream re-enables it in its OWN manifest's allowlist**, because
an `import:`ed manifest composes rather than inheriting ours as a ceiling.

### Proven by build, not by grep

Both leaves built against the narrowed manifest, in a west topdir where
`hal_nxp`, `hal_stm32` and `hal_nordic` are absent from disk as well as from the
manifest — `west list` returns 10 projects instead of 13:

* `examples/zephyr/rust/talker`, `native_sim/native/64`, zenoh — **built**
* `examples/zephyr/c/talker`, `mps2_an385`, zenoh — **built**

### What is STILL paid: `hal_espressif`, 275 MB

Questions (1) and (2) above were measured and both came back empty — nothing
builds an esp32 image through Zephyr, and `hal_espressif` has no consumer of any
kind (details in issue 1282). By this issue's step 2 that would remove it.

**It was kept anyway.** RFC-0099 D10 and phase-447 F1 both say the third
question — whether Zephyr's espressif support makes our ESP-IDF provisioning a
duplicate — is a platform-strategy question to measure, not to settle while
deleting a manifest line, and deleting it settles it in the direction of "we do
not do Zephyr esp32". That question is **issue 1282**; when it is answered this
entry either gains a `needed_by` or gets `lines = []`.

So: of the 2.5 GB this issue measured, **~2.29 GB is no longer fetched** by a
fresh workspace and **275 MB knowingly still is**, with the index saying so in
as many words rather than hiding it.

### And it reclaims nothing you already have

`west update` does not prune. This is a fresh-workspace saving; a reader who
measures their own populated tree and sees no change has not found a bug. Both
the index comment and the `--dry-run` output say so, because this issue warned
that a reader who missed it would revert the change.
