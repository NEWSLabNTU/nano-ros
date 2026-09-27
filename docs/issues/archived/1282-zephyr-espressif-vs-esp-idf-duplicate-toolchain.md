---
id: 1282
title: "`hal_espressif` has no measured consumer and is retained anyway — is our
  separate ESP-IDF provisioning a duplicate of Zephyr's own espressif support?"
status: resolved
type: tech-debt
area: build, esp32
severity: low
found: 2026-09-11
related: [issue-1275, issue-0500, phase-468]
resolved: 2026-09-27
resolved_by: maintainer decision — bare-metal esp-hal is the esp32 path
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

### Amended 2026-09-25 (phase-468 W2): the premise above is half false now, and the issue STAYS OPEN

phase-468 W2 retired the ESP-IDF port. `scripts/esp_idf/setup.sh`,
`just/esp_idf.just`, `packages/platform/nros-platform-esp-idf/`,
`integrations/nano-ros/` and the `esp_idf` cmake vocabulary are all gone, the
book stopped offering it, and step 7 deleted the last environment knobs
(`NROS_ESP_IDF_WORKSPACE`, `NROS_ESP_IDF_ENV_SHIM`, `IDF_PATH`) along with the
`esp-idf-workspace` root they named.

**So the duplication framing is settled, by subtraction rather than by
argument.** There is no separate ESP-IDF provisioning any more, so we do not
carry two toolchains for one target. The paragraph above describing that state
is history, not the present.

**What is NOT settled, and is why this stays open.** Nothing measured about
`hal_espressif` itself changed. Re-checked on this branch: still zero consumers
(the west allowlists and prose only), still no `xtensa-espressif_*` toolchain in
what `scripts/zephyr/setup.sh` installs, still the unreachable
`CONFIG_SOC_SERIES_ESP32C3` branch in `zephyr/cmake/nros_cargo_build.cmake`
(now line 84; the `:78` cited above is where F1 found it), still 275 MB on every
fresh `west update`. Measured the same way F1 did: `hal_espressif` appears in
exactly three non-prose places, `nros-sdk-index.toml` and the two west
allowlists, and none of them is a build. The reason F1 kept it was that
deleting the line would settle a strategy question by accident, and that reason
survives the retirement — it has only changed shape:

* **before:** ESP-IDF vs Zephyr — do we carry two host ecosystems for esp32?
* **now:** esp-hal bare-metal vs Zephyr — should the one remaining esp32 path
  gain a Zephyr-hosted sibling, or is bare-metal the whole answer?

That is a narrower question and a genuinely open one. Retiring ESP-IDF removed a
competitor, which if anything makes a Zephyr esp32 path *easier* to argue for,
not harder — so deleting `hal_espressif` now would still settle it in the
direction of "we do not do Zephyr esp32", which is exactly the accident this
issue exists to prevent.

The closing conditions below are unchanged and still correct; only the second
bullet's rival has changed name, from ESP-IDF to esp-hal.

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

## Resolution — 2026-09-27: the DECISION, taken deliberately

**The user settled the open question: bare-metal esp-hal is the esp32 path for
now, and we do not support Zephyr-hosted esp32.** `hal_espressif` is therefore
dropped, and this issue closes.

What matters here is that this is a decision and not a measurement running out.
Everything measurable had already come back empty twice — phase-447 F1 on
2026-09-11 and phase-468 W2 on 2026-09-25 — and both times the module was KEPT,
because a deletion would have answered a strategy question as a side effect of
tidying a manifest. That reasoning was right, and it is exactly why it no longer
applies: the question was put to the person whose call it is, and answered.

**Re-measured before deleting, the same way F1 did.** A tree-wide grep for
`hal_espressif` returns exactly three non-prose hits — the
`[zephyr_module.hal_espressif]` entry in `nros-sdk-index.toml` and the two west
allowlists — and everything else is documentation prose. No `prj.conf`,
overlay, board crate, fixture row, CI job or `west build -b` names an Espressif
board. The premise held, so the decision was taken on current facts.

### What landed

* `[zephyr_module.hal_espressif]` keeps its entry with `lines = []`, the shape
  `hal_nxp` / `hal_stm32` / `hal_nordic` already use. The entry is the record of
  the decision; deleting it outright would make the removal a line that vanished
  rather than a decision a reader can find.
* Removed from both west allowlists (`west.yml`, `west-4.4.yml`), which
  `check-zephyr-module-allowlist` asserts in both directions.
* The dead `elseif(CONFIG_SOC_SERIES_ESP32C3)` Rust-target branch in
  `zephyr/cmake/nros_cargo_build.cmake` is deleted. Removal is clean:
  `CONFIG_SOC_SERIES_ESP32C3` occurred exactly once in the tree, and the
  function's `else()` arm already handles an unmodeled SoC by naming the host
  triple with a warning. The triple it set, `riscv32imc-unknown-none-elf`, is
  still declared by the bare-metal esp-hal leaves, so
  `check-rust-targets-covered` still wants its row.
* `CLAUDE.md`'s "knowingly kept" line corrected — it had become false.

### Reversing this

Re-adding the module is not sufficient on its own, and that is the honest cost
of the decision rather than an argument against reversing it. A Zephyr esp32
image needs three things, none of which exists today:

1. `lines = ["3.7", "4.4"]` back on `[zephyr_module.hal_espressif]`, mirrored
   into both allowlists;
2. a board — no board dir, crate, conf or fixture row names an Espressif board;
3. an `xtensa-espressif_*` toolchain in `scripts/zephyr/setup.sh`, which today
   installs only `x86_64-zephyr-elf` and `arm-zephyr-eabi` (plus
   `aarch64-zephyr-elf` behind a flag). Without it the image could not link even
   with the module present — which is why the branch deleted above was
   unreachable rather than merely unused.

A Rust-target branch would come back with the board, from the same evidence.

### What this does NOT reclaim

`west update` NEVER PRUNES. An existing `zephyr-workspace` keeps its 275 MB
until it is re-fetched, so a reader who measures one sees no change; the saving
is on a FRESH workspace fetch. This is the same caveat issue 1275 recorded for
the other 2.29 GB, and it is worth restating because the natural way to check
this change is the one way that cannot show it.
