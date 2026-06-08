---
rfc: 0013
title: "Custom-board provisioning — out-of-tree boards self-describe their deps"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Custom-board provisioning — out-of-tree boards self-describe their deps

**Status.** Design exploration (2026-05-29). Follow-up to Phase 197 (nros setup
as the single provisioning entrypoint) + the build-side design in
[`0012-board-bsp-integration-architecture.md`](0012-board-bsp-integration-architecture.md).

**Problem.** Today the SDK index (`nros-sdk-index.toml`) `[board.*]` table is the
**maintainer-owned** registry: `nros setup <board>` looks a board up there to learn
its `packages` (tools + sources). A **user who authors their own board crate** has
no index entry, so `nros setup <their-board>` can't know what to provision. Phase
197 made `nros setup <board>` the complete provisioner *for nano-ros's own boards*;
this doc extends it to **out-of-tree boards that self-describe their deps**.

The build-side already supports out-of-tree boards (a generic board crate + a
~50-LOC vendor overlay the user authors — see the layered model in the BSP doc).
The board crate also already self-describes its **build config** via
`nros-board.toml` (Phase 195.C: `cargo_config` with `${workspace}` substitution,
`toolchain`, `platform_feature`, `[board.entry]`). The missing piece is the
**provisioning** half: the board crate declaring the *source/tool deps* its build
needs so `nros` can fetch them.

---

## Real-board survey (what real deps look like)

Surveyed 2026-05-29 across four segments to ground the schema in real provisioning
shapes (not just nano-ros's qemu boards):

| Segment | Representative boards | MCU/SoC | Dep-provisioning shape |
|---|---|---|---|
| **Maker** | RP2040/Pico, RP235x, ESP32-C3/S3 devkits, STM32 Nucleo/Disco, Teensy | Cortex-M0+/M4F, RISC-V, Xtensa | **cargo crate** HAL (`rp2040-hal`, `esp-hal`, `stm32f4xx-hal`, `embassy-*`) + rustup target + a **runner tool** (`picotool`/`probe-rs`/`elf2uf2`). C side only for ESP-IDF. |
| **Industrial** | BeagleBone (AM335x), RPi CM4/5, NXP i.MX8, TI Sitara | Cortex-A + Cortex-M (AMP) | **git Yocto layers** (`meta-raspberrypi`, `meta-ti`, `meta-freescale`) **or** a **vendor SDK download** (TI Processor-SDK, NXP i.MX Linux BSP). Optional RTOS (TI-RTOS/FreeRTOS) on the Cortex-M side. |
| **Automotive** | NXP S32G/S32K, Infineon AURIX TriCore, NVIDIA DRIVE Orin/Thor, Renesas R-Car/RH850 | lockstep Cortex-R + A + AI SoC | **vendor SDK, often license-gated** (NVIDIA DriveOS/JetPack, S32 Design Studio, AURIX iLLD). Vendor/AUTOSAR toolchains (TASKING for TriCore, GHS) — not arm-gcc. A supervisory MCU (AURIX/RH850) pairs the AI SoC. |
| **Drone** | Pixhawk FMUv6X/6C, Holybro, CUAV, ARK, ModalAI VOXL 2, NXP RDDRONE | STM32H7 (NuttX) + companion SoC (QRB5165) | **git** PX4-Autopilot (NuttX, FMUv6 board defs in-tree, ~50 nested submodules) + vendor **companion SDK** (ModalAI VOXL SDK = git). arm-gcc toolchain. |

**Dep-provisioning taxonomy** (maps 1:1 onto nros's existing index source kinds):

1. **cargo dependency** (maker Rust HALs) — *cargo already fetches these*; nros
   provisions nothing, just declares the rustup target + runner tool.
2. **git clone/submodule @ ref** — Yocto layers, PX4-Autopilot, ModalAI SDK,
   git-pinned HAL forks → nros `[source.*]` (submodule/dest/ref).
3. **prebuilt or source-built host tool** — `picotool`, `probe-rs`, `idlc`, a
   vendor SDK with a free download → nros `[tool.*]` (dist or `[tool.*.source]`).
4. **license-gated** — DriveOS, JetPack, AURIX/TASKING → nros `[gated.*]`
   (env-var pointed; `nros doctor` checks, never downloads).

The four kinds **already exist** in `nros-sdk-index.toml`. The only thing missing
is letting a *board crate* carry them inline instead of the central index.

---

## The gap, precisely

- `nros-board.toml` (in the board crate) declares **build config** ✔ but **no deps**.
- `nros-sdk-index.toml` declares **deps** ✔ but is **maintainer-owned** (a user can't
  add `[board.my-board]` to a file they don't own).
- The released `nros` parses `[board.*]`/`[rmw.*]` with a **strict schema** (Phase
  197.4 finding) — so a self-describing board needs a *defined* schema, not ad-hoc
  fields.

## Proposed schema — `nros-board.toml` carries inline deps

Extend the board crate's `nros-board.toml` (the file nros already reads) with an
optional dep block per board. Same vocabulary as the index, so the resolver is
shared:

```toml
[[board]]
names = ["my-rover"]
platform = "freertos"
toolchain = "stable"
board_crate = "my-rover-bsp"
cargo_config = '''
[build]
target = "thumbv7em-none-eabihf"
[target.thumbv7em-none-eabihf]
runner = "probe-rs run --chip STM32H743ZITx"
rustflags = ["-C", "link-arg=-Tlink.x"]
[patch.crates-io]
my-vendor-hal = { path = "${workspace}/external/my-vendor-hal" }
'''

# NEW (custom-board provisioning) — deps this board needs, fetched by
# `nros setup <board>` exactly like an index board's `packages`. Each kind reuses
# the index [source.*]/[tool.*]/[gated.*] grammar.
[[board.source]]            # git clone/submodule @ ref
name = "my-vendor-hal"
git = "https://github.com/acme/my-vendor-hal.git"
ref = "v1.4.0"
dest = "external/my-vendor-hal"   # ${workspace}-relative; the cargo_config patch points here

[[board.tool]]              # host tool: prebuilt dist OR source-built OR cargo-install
name = "probe-rs"
cargo_install = "probe-rs-tools"  # = `cargo install probe-rs-tools` (maker runner)

[[board.gated]]            # license-gated vendor SDK (never downloaded)
name = "driveos"
env = "NV_DRIVEOS_DIR"
hint = "Install NVIDIA DriveOS via SDK Manager; export NV_DRIVEOS_DIR"
```

Resolution rule for `nros setup <board>`:
1. If `<board>` is in the **central index** `[board.*]` → today's path (nano-ros's
   own boards).
2. **Else** discover the board crate (by name in the workspace, or `--board-manifest
   <path>`), read its `nros-board.toml`, and provision its `[[board.source]]` /
   `[[board.tool]]` / `[[board.gated]]` — then write the `cargo_config` with
   `${workspace}` → the provisioned `dest` paths. *(That last step — "nros prepares
   the config files required by the board crate" — already exists for in-tree
   boards; this reuses it.)*

So the central index becomes the registry for **nano-ros's own** boards; **user
boards self-describe** in their own crate. One resolver, two sources of truth that
never overlap (a board id is in exactly one).

## nros-cli changes required

- `SdkIndex`/board resolution: accept a board descriptor from a **board crate's
  `nros-board.toml`** (not only the central index), with the `[[board.source]]` /
  `[[board.tool]]` / `[[board.gated]]` blocks above. (Schema addition — the 197.4
  strict-parse finding means this must be a real field, shipped in a release.)
- `cargo_install` as a new `[[board.tool]]` provisioning kind (maker runners:
  `probe-rs`, `picotool`, `elf2uf2-rs`) — `cargo install <pkg>` into the nros store.
- `nros setup --board-manifest <path>` (or board-name discovery in the workspace) to
  point at an out-of-tree board crate.
- Doc the precedence (central index wins for nano-ros boards; crate manifest for the
  rest) + the "exactly one home" invariant.

---

## Simulated setup walkthrough (custom board, out-of-tree)

A maker authoring an STM32H7 "my-rover" board crate, no central-index entry:

```text
# 1. scaffold a board crate (or hand-write) with an nros-board.toml carrying the
#    [[board.source]]/[[board.tool]] blocks above.
$ nros new --board my-rover --platform freertos --kind vendor-module   # (future flag)

# 2. provision — nros reads the crate's nros-board.toml, fetches its deps:
$ nros setup my-rover
nros setup: my-rover (board crate my-rover-bsp) needs 2 dep(s):
  my-vendor-hal   source v1.4.0 — git → external/my-vendor-hal        [provisioned]
  probe-rs        tool          — cargo install probe-rs-tools         [provisioned → ~/.nros/sdk/probe-rs/...]
nros setup: wrote .cargo/config.toml (${workspace} → external/my-vendor-hal)   # nros prepares the config

# 3. host-env (rustup target) + build + run — same as any nano-ros board:
$ source ./setup.bash      # PATH the nros-store tools (probe-rs) — Phase 197.4
$ nros build               # cargo cross-build, my-vendor-hal patched in
$ nros deploy my-rover     # probe-rs run --chip … (the cargo_config runner)
```

**Current state of the simulation (what works vs needs the nros-cli change):**
- The board crate + its `nros-board.toml` build config (`cargo_config`,
  `${workspace}`) — **works today** (Phase 195.C reads it for in-tree boards).
- `[[board.source]]`/`[[board.tool]]`/`cargo_install` provisioning + reading the
  descriptor from a crate (not the central index) — **needs the nros-cli schema +
  resolver change** above (the released nros only resolves central-index boards).
- The dep kinds themselves (git source, dist/source tool, gated) — **already
  implemented** in nros provisioning; this is wiring, not new mechanism.

A concrete fixture (`external/rp-hal` cloned for study; a sample
`external/sim-board/my-rover-bsp` crate carrying the `nros-board.toml` above) can
exercise steps 1+3 today; step 2 is the gated nros-cli work. **Captured today
(nros 0.3.1)** — the gap, verbatim:

```text
$ nros setup my-rover --dry-run
Error: nros setup: unknown board 'my-rover'. Known boards: esp32, …, stm32f4,
threadx-linux, zephyr. Add a [board.my-rover] entry to nros-sdk-index.toml.
   at nros-cli-core/src/cmd/setup.rs:444
```

i.e. `nros setup [BOARD]` resolves **only** the central index `[board.*]` table;
there is no `--board-manifest` / board-crate descriptor path yet. That single
resolver branch (index miss → read the board crate's `nros-board.toml`) is the
crux of the nros-cli change.

---

## Open questions

- **cargo vs nros deps.** Maker boards' HALs are *cargo* deps (crates.io) — should
  the board crate just use `[dependencies]` (cargo fetches) and reserve
  `[[board.source]]` for non-cargo git/vendor trees? (Leaning yes — don't duplicate
  cargo's job; `[[board.source]]` is for the things cargo can't pull.)
- **Discovery.** Board-name → crate: scan `packages/boards/**` + a user-config search
  path, or require `--board-manifest`? A registry-of-manifests vs convention.
- **Versioning the board schema** so an older `nros` degrades gracefully (the 197.4
  deny-unknown-fields lesson — version the descriptor or use `#[serde(default)]`).
- **Gated SDKs in CI** stay out (license) — only `nros doctor` checks the env var,
  matching today's `[gated.*]`.

## Phase proposal

Implementable as a new phase (next free id ~201): (1) board-descriptor schema +
resolver in nros-cli (read deps from a crate's `nros-board.toml`); (2) `cargo_install`
tool kind; (3) `--board-manifest` / board discovery; (4) a `nros new --board`
scaffolder; (5) a sample out-of-tree board crate fixture + an acceptance lane
mirroring the Phase 195 fresh-machine gate but for a self-describing board.

## Sources
- [PX4/Pixhawk FMUv6 open standards](https://dronecode.org/pixhawk-fmuv6-family-of-open-standards-are-now-available/) · [ModalAI VOXL 2 (PX4)](https://docs.px4.io/main/en/flight_controller/modalai_voxl_2)
- [NXP S32 Automotive Platform](https://www.nxp.com/products/processors-and-microcontrollers/s32-automotive-platform:S32) · [NVIDIA DRIVE Orin](https://www.nevsemi.com/blog/my-deep-dive-into-nvidia-drive-orin-the-brain-of-autonomous-vehicles)
- [rp-hal (RP2040 Rust HAL)](https://github.com/rp-rs/rp-hal) · [stm32f4xx-hal](https://github.com/stm32-rs/stm32f4xx-hal) · [embedded-hal](https://github.com/rust-embedded/embedded-hal)
- [Yocto BSP guide](https://docs.yoctoproject.org/3.2.3/bsp-guide/bsp.html) · [TI Processor-SDK AM335x](https://www.ti.com/tool/PROCESSOR-SDK-AM335X) · [NXP i.MX Embedded Linux](https://www.nxp.com/design/design-center/software/embedded-software/i-mx-software/embedded-linux-for-i-mx-applications-processors:IMXLINUX)
