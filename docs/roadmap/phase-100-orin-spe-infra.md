# Phase 100 — AGX Orin SPE infrastructure (Cortex-R5F + IVC)

**Goal:** ship the platform-level support nano-ros needs to run on NVIDIA Jetson AGX Orin's
**Sensor Processing Engine (SPE)** — a Cortex-R5F core running NVIDIA's FreeRTOS V10.4.3
FSP. Application-level work (porting the safety-island packages from the friend project
[`autoware_sentinel`](https://github.com/jerry73204/autoware_sentinel)) lives in that
repo's [Phase 11](https://github.com/jerry73204/autoware_sentinel/blob/main/docs/roadmap/11-orin-spe.md);
this phase delivers the pieces nano-ros has to provide to make Phase 11 buildable.

**Status:** Done. All 10 sub-items landed; POSIX mock-IVC E2E green; hardware bring-up
deferred to `autoware_sentinel` Phase 11.7.
**Priority:** Medium (driven by autoware_sentinel Phase 11 dependency).
**Depends on:** none in nano-ros (greenfield).
**Cross-cutting:** `autoware_sentinel` Phase 11 consumes everything this phase produces.

## Background

The SPE is the always-on safety MCU on AGX Orin. It boots before the CCPLEX, runs
independently of Linux, and survives Linux crashes — making it the natural home for the
heartbeat watchdog, MRM (Minimum Risk Manoeuvre) handler, emergency-stop operator,
control validator, and vehicle-command gate that `autoware_sentinel` already implements
in `no_std` Rust.

| Aspect | Existing nano-ros (MPS2-AN385 FreeRTOS) | AGX Orin SPE | Gap |
|--------|------------------------------------------|----------------|-----|
| CPU | Cortex-M3 (ARMv7-M) | Cortex-R5F (ARMv7-R) | New ISA family |
| Target triple | `thumbv7m-none-eabi` | `armv7r-none-eabihf` | Add to all build chains |
| FreeRTOS port | `portable/GCC/ARM_CM3` | `portable/GCC/ARM_CR5` (NVIDIA FSP) | Different port, vendor-shipped |
| Critical section | PRIMASK (1-bit) | CPSR I-bit | Abstract behind feature flag |
| Heap budget | ~256 KB (16 MB SRAM) | ~30 KB (256 KB BTCM) | Shrink config dramatically |
| Networking | LAN9118 + lwIP/smoltcp + zenoh-pico TCP/UDP | None — only IVC mailboxes to CCPLEX | New link transport |
| Float ABI | hard (`thumbv7em-none-eabihf`) or soft | NVIDIA BSP uses softfp; choose carefully | Resolve at link time |

The single largest piece is **transport**: zenoh-pico, XRCE-DDS, and dust-DDS all assume
UDP/TCP. The SPE has no Ethernet. The only viable transport is **IVC** — DRAM carveout
ring buffers signalled via HSP (Hardware Synchronization Primitives) doorbells. Rather
than introduce a new RMW backend, this phase adds an **IVC link transport inside
zenoh-pico** (`Z_FEATURE_LINK_IVC`), so `nros-rmw-zenoh` works unchanged on the SPE.
That's a smaller surface than a full new RMW (precedent: `Z_FEATURE_LINK_RAWETH`, the
SPI/serial link family).

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│  CCPLEX (Cortex-A78AE, Linux)                              │
│                                                            │
│  Autoware ── rmw_zenoh_cpp ──► zenohd ◄── tcp ──► IVC      │
│                                  ▲                bridge   │
│                                tcp:7447          daemon    │
│                                                    │       │
│                                                /dev/tegra-ivc  │
└────────────────────────────────────────────────────────────┘
                  │ DRAM carveout (16 frames × 64 B/channel)  │
                  │      HSP doorbell (interrupt)             │
┌────────────────────────────────────────────────────────────┐
│  SPE (Cortex-R5F, NVIDIA FreeRTOS V10.4.3 FSP)             │
│                                                            │
│  nros-board-orin-spe (run loop, FreeRTOS task)             │
│      │                                                     │
│      ▼                                                     │
│  nros-platform-freertos [cortex-r feature]                 │
│      │                                                     │
│      ▼                                                     │
│  zpico-platform-shim::ivc_helpers → zenoh-pico             │
│      │            (forwards via <P as PlatformIvc>::*)     │
│      ▼                                                     │
│  nros-platform-orin-spe (impl PlatformIvc, Clock, …)       │
│      │                                                     │
│      ▼                                                     │
│  packages/drivers/nvidia-ivc (driver crate)                │
│      │      ├─ feature `fsp`        → tegra_ivc_channel_*  │
│      │      └─ feature `unix-mock`  → Unix-socket pair     │
│      ▼                                                     │
│  Z_FEATURE_LINK_IVC inside vendored zenoh-pico             │
│  (new C link transport — peer to TCP/UDP/Serial/RawEth)    │
│                                                            │
│  Application: autoware_sentinel reduced subset             │
└────────────────────────────────────────────────────────────┘
```

### Layering rules

- **`packages/drivers/nvidia-ivc/`** is a self-contained driver crate. No
  dep on `nros-platform`, `nros-rmw`, or zenoh-pico. Two compile-time
  backends behind features: `fsp` (links NVIDIA's `tegra_aon_fsp.a` via
  `NV_SPE_FSP_DIR`) and `unix-mock` (Unix-domain-socket pair, Linux-only,
  for POSIX dev + CI). Reusable by any other Tegra Cortex-R5/R52 project.
- **`packages/platforms/nros-platform-orin-spe/`** is a thin trait-impl
  crate. Implements `PlatformIvc`, `PlatformClock`, `PlatformSleep`,
  `PlatformAlloc`, `PlatformThreading`. Most impls re-export from
  `nros-platform-freertos`; `PlatformIvc` delegates to the driver.
- **`zpico-platform-shim::ivc_helpers`** (gated on a new `ivc` feature)
  exposes `_z_open_ivc` / `_z_read_ivc` / `_z_send_ivc` / `_z_close_ivc`
  / `_z_ivc_notify` symbols that zenoh-pico's `Z_FEATURE_LINK_IVC` C code
  consumes. Forwards to `<P as PlatformIvc>`.
- **Sentinel's Linux-side IVC bridge daemon** can also depend on
  `nvidia-ivc` (`unix-mock` for testing, plain sysfs read/write on real
  Orin), keeping a single Rust API both sides of the wire.

## Work items

- [x] **100.0 — `nvidia-ivc` driver crate**

      Create `packages/drivers/nvidia-ivc/` — a self-contained NVIDIA Tegra
      IVC driver. No `nros-platform` / `nros-rmw` / zenoh-pico deps. Mirrors
      the existing `packages/drivers/{lan9118-smoltcp,openeth-smoltcp,…}`
      pattern: vendor-chip glue lives in `drivers/`, platform crates wire it
      into the trait stack.

      **Public API (safe Rust):**
      ```rust
      pub struct Channel(/* opaque */);
      impl Channel {
          pub fn open(id: u32) -> Option<Self>;
          pub fn read(&self, buf: &mut [u8])  -> Result<usize, IvcError>;
          pub fn write(&self, buf: &[u8])     -> Result<usize, IvcError>;
          pub fn notify(&self);
          pub fn frame_size(&self) -> usize;
      }
      ```
      Plus C-callable `extern "C"` wrappers (`nvidia_ivc_channel_*`) consumed
      by zenoh-pico's `Z_FEATURE_LINK_IVC` C code and by the shim.

      **Cargo features (mutually exclusive):**
      - `fsp` — links `tegra_aon_fsp.a` via `NV_SPE_FSP_DIR` env. `no_std`.
      - `unix-mock` — Unix-domain-socket pair simulating one IVC channel
        (Linux-only, requires `std`). Used by 100.5 + sentinel CI.
      - `std` — pulled in by `unix-mock`.

      **Files:**
      - `packages/drivers/nvidia-ivc/Cargo.toml`
      - `packages/drivers/nvidia-ivc/build.rs` (cfg(fsp) → link search)
      - `packages/drivers/nvidia-ivc/src/lib.rs` (safe API + dispatch)
      - `packages/drivers/nvidia-ivc/src/fsp.rs` (cfg(fsp) — extern "C" decls)
      - `packages/drivers/nvidia-ivc/src/unix_mock.rs` (cfg(unix-mock))
      - `packages/drivers/nvidia-ivc/src/error.rs`
      - `packages/drivers/nvidia-ivc/tests/loopback.rs` (unix-mock loopback)
      - `packages/drivers/nvidia-ivc/README.md` (NV SDK Manager prereq + EULA)

      Crate is added to `[workspace.exclude]` (matches `nros-board-orin-spe`
      pattern) because `fsp` builds need a user-supplied SDK path.

      **Acceptance:**
      - `cargo build -p nvidia-ivc --features unix-mock` succeeds on Linux.
      - `cargo test -p nvidia-ivc --features unix-mock` runs the loopback
        test (open two channels, exchange frames, assert reassembly).
      - `cargo build -p nvidia-ivc --features fsp --target armv7r-none-eabihf
        -Zbuild-std=core` succeeds when `NV_SPE_FSP_DIR` is set.

- [x] **100.0a — `PlatformIvc` trait in `nros-platform-api`**

      Add an opaque-pointer trait alongside `PlatformTcp` / `PlatformUdp` /
      `PlatformSocketHelpers`:
      ```rust
      pub trait PlatformIvc {
          fn channel_get(id: u32) -> *mut c_void;
          fn read(ch: *mut c_void,  buf: *mut u8,  len: usize) -> isize;
          fn write(ch: *mut c_void, buf: *const u8, len: usize) -> isize;
          fn notify(ch: *mut c_void);
          fn frame_size(ch: *mut c_void) -> u32;
      }
      ```
      No impl in this sub-item — just the contract. ~50 LOC.

      **Acceptance:** `nros-platform-api` builds for `thumbv7m-none-eabi`
      and `armv7r-none-eabihf` without changes elsewhere.

- [x] **100.1 — Cortex-R5 critical-section abstraction in `nros-platform-freertos`**

      Currently `packages/core/nros-platform-freertos` hard-codes Cortex-M PRIMASK for
      its critical-section primitives. Add a `cortex-r` feature that swaps the inline
      asm for ARMv7-R CPSR I-bit toggling (`mrs r0, cpsr; cpsid i; … msr cpsr_c, r0`),
      and gate the existing `cortex-m` path behind a `cortex-m` feature.

      **Files:**
      - `packages/core/nros-platform-freertos/src/lib.rs` (critical-section impl)
      - `packages/core/nros-platform-freertos/Cargo.toml` (feature flags)

      **Acceptance:** crate builds for both `thumbv7m-none-eabi` (existing MPS2 path) and
      `armv7r-none-eabihf` (new SPE path). No behavioural change on Cortex-M.

- [x] **100.2 — `armv7r-none-eabihf` target in workspace toolchain wiring**

      The Rust target is Tier 3, supported by rustc since 1.49 but not pre-built.
      Requires `build-std` for `core`/`alloc` and a `rust-toolchain.toml`-pinned
      nightly. Wire into:

      - `tools/rust-toolchain.toml` (add `armv7r-none-eabihf` to `targets`)
      - `just/workspace.just` `rust-targets` recipe
      - `Cargo.toml` `[unstable]` build-std-features in the new board crate's
        `.cargo/config.toml`

      **Acceptance:** `cargo +nightly build --target armv7r-none-eabihf -Zbuild-std=core,alloc -p nros-platform-freertos` succeeds.

- [x] **100.3 — `zpico-platform-shim` Cortex-R5 build**

      The shim's symbol forwarding is platform-agnostic but its `cc` build script and
      no-default-features set assume Cortex-M defaults. Verify the existing `active`
      feature path works on `armv7r-none-eabihf` with a Cortex-R `ConcretePlatform`,
      and that the `network` feature stays optional (SPE has no UDP/TCP — pulls only
      the `_z_socket_*` no-op stubs added in commit 12c2bfe3).

      **Files:**
      - `packages/zpico/zpico-platform-shim/build.rs` (target detection)
      - `packages/zpico/zpico-platform-shim/src/shim.rs` (already-gated network code)

      **Acceptance:** shim compiles for `armv7r-none-eabihf`. `_z_socket_*` stubs
      resolve cleanly without a network impl.

- [x] **100.4 — Vendored zenoh-pico: `Z_FEATURE_LINK_IVC`** ✓

      Landed via fork commits on the `nano-ros-phase-100-link-ivc` branch of
      `jerry73204/zenoh-pico`, with the parent submodule pointer pinned at
      `897618d5` (covers the base `Add Z_FEATURE_LINK_IVC link transport`
      commit `3243086b` + Phase 11.3.A zero-copy IVC + Phase 11.3.B
      ZENOH_ORIN_SPE platform header). Companion infrastructure (parent
      `build.rs` `LinkFeatures::ivc`, `zpico-sys` `link-ivc` cargo feature,
      `zpico-platform-shim::ivc_helpers` forwarders) was already wired
      bottom-up during 100.3 / 100.5 work.

      **Acceptance verified:**
      - `Z_FEATURE_LINK_IVC=0` default build is byte-identical to current
        (no new symbols, no new `.o` files, every gated `#if` dead).
      - `Z_FEATURE_LINK_IVC=1` builds on POSIX (validated by the orin_spe
        mock test below) and on `armv7r-none-eabihf` (gated by the
        SPE board crate's CMake / cc Build paths).
      - **Reassembly tests (`nros-tests::orin_spe_mock_ivc`, 4/4 PASS)
        via the `nvidia-ivc` `unix-mock` backend:**
          • `single_frame_message_round_trips`
          • `multi_frame_zenoh_batch_reassembles`
          • `keepalive_ping_is_dropped_silently`
          • `wire_violation_yields_protocol_error`
        Run via `just orin_spe test`.

- [x] **100.5 — `nros-platform-orin-spe` (platform crate)**

      Create `packages/platforms/nros-platform-orin-spe/`. Thin trait-impl crate
      that wires the SPE's HAL into the standard `nros-platform-api` traits.
      Layout matches the existing `nros-platform-{mps2-an385,stm32f4,esp32,…}`
      siblings.

      **Trait impls:**
      - `PlatformIvc` — delegates to `nvidia-ivc` (with `fsp` feature on hardware
        builds, `unix-mock` feature on POSIX dev).
      - `PlatformClock` — re-export from `nros-platform-freertos` (FSP exposes
        the same FreeRTOS V10.4.3 tick API).
      - `PlatformSleep` — `vTaskDelay`. Re-export.
      - `PlatformAlloc` — FSP's `pvPortMalloc` / `vPortFree` (heap_4 in FSP).
      - `PlatformThreading` — `xTaskCreate` etc. Re-export.
      - `PlatformRandom` — best-effort `rand_r`-equivalent or a hash-of-tick
        fallback (note: SPE has no hardware RNG; document the weakness).

      **Files:**
      - `packages/platforms/nros-platform-orin-spe/Cargo.toml`
      - `packages/platforms/nros-platform-orin-spe/src/lib.rs` (re-exports +
        `pub struct OrinSpe;` as `ConcretePlatform`)
      - `packages/platforms/nros-platform-orin-spe/src/ivc.rs` (PlatformIvc impl)
      - `packages/platforms/nros-platform-orin-spe/src/random.rs` (RNG note)

      Wire into `nros-platform/Cargo.toml`:
      ```toml
      [features]
      platform-orin-spe = ["dep:nros-platform-orin-spe"]
      [dependencies]
      nros-platform-orin-spe = { path = "../../platforms/nros-platform-orin-spe", optional = true }
      ```

      **Acceptance:** `cargo build -p nros-platform --no-default-features
      --features platform-orin-spe --target armv7r-none-eabihf
      -Zbuild-std=core,alloc` succeeds (with `NV_SPE_FSP_DIR` set).

- [x] **100.6 — `nros-board-orin-spe` board crate**

      New crate at `packages/boards/nros-board-orin-spe/`. Mirrors the
      `nros-board-mps2-an385-freertos` shape (`Config`, `run<F>`, `println!` macro)
      but:
      - links against NVIDIA FSP static libs instead of building FreeRTOS from source
        (the FSP ships the prebuilt `ARM_CR5` port; we don't recompile it)
      - exposes `tegra_ivc_channel_*` as the IVC transport vtable consumed by 100.4
      - drops every Ethernet/lwIP/LAN9118 dep
      - sets `Config::zenoh_locator` default to `ivc/2` (channel 2 = aon_echo)
      - `println!` writes to TCU via FSP's `printf`
      - links nano-ros statically into the SPE firmware image (`libnros_orin_spe.a` →
        NVIDIA Makefile via `ENABLE_NROS_APP := 1` template flag)

      **Files (new):**
      - `packages/boards/nros-board-orin-spe/Cargo.toml`
      - `packages/boards/nros-board-orin-spe/src/lib.rs` (run, Config, println)
      - `packages/boards/nros-board-orin-spe/src/ivc.rs` (vtable wiring)
      - `packages/boards/nros-board-orin-spe/build.rs` (NVIDIA FSP path resolution +
        link search)
      - `packages/boards/nros-board-orin-spe/.cargo/config.toml`
        (target = `armv7r-none-eabihf`, build-std)
      - `packages/boards/nros-board-orin-spe/README.md` (NVIDIA SDK Manager prereqs,
        env vars: `NV_SPE_FSP_DIR`, `NV_TEGRA_VERSION`)

      The crate is **excluded from the workspace** (lives in `[workspace.exclude]`)
      because the rest of the workspace can't see the NVIDIA FSP. It builds via
      `just orin_spe build` against an env-pointed FSP install.

      **Acceptance:**
      - `just orin_spe build` produces `spe.bin`.
      - `arm-none-eabi-size spe.bin` reports `.text + .data + .bss < 256 KB`.
      - Float ABI consistent across Rust + C objects (no `attribute Tag_ABI_VFP_args`
        warnings at link time).

- [x] **100.7 — Justfile + setup wiring**

      Added `just/orin-spe.just` mod with the standard recipe set: `setup`,
      `doctor`, `build`, `build-fixtures`, `test`, `flash`, `ci`, `clean`.

      - `setup` installs the `armv7r-none-eabihf` target on the workspace
        nightly and prints a notice for `NV_SPE_FSP_DIR`. The FSP itself
        ships under NVIDIA's SDK Manager EULA and cannot be auto-installed.
      - `doctor` is read-only. Exits 1 if the target or `arm-none-eabi-gcc`
        is missing; treats `NV_SPE_FSP_DIR` as informational so host CI
        without the SDK passes.
      - `build` runs `cargo +nightly build --release` inside the (workspace-
        excluded) board crate against `NV_SPE_FSP_DIR`. Fails loud if the env
        var isn't set, with a pointer to `just orin_spe test` for the POSIX
        path that doesn't need the SDK.
      - `flash` uses the L4T BSP's `flash.sh -k A_spe-fw …` against an x86
        host in USB recovery mode. Reads `L4T_BSP_DIR`; refuses to run
        without it.
      - `bsp-download` / `bsp-build` / `bsp-clean` (added post-100.8) fetch
        NVIDIA's SPE FreeRTOS BSP from L4T 36.4.4's `public_sources.tbz2`
        + the ARM GNU 13.2 toolchain, build the upstream
        `rt-aux-cpu-demo-fsp` Makefile, and repackage the resulting `.o`
        files into a single `libtegra_aon_fsp.a` (~37 MB on t23x) plus a
        staged `include/` tree under `external/spe-fsp/install/`. `build`
        auto-detects the staged install when `NV_SPE_FSP_DIR` is unset.
        Downloads are idempotent (cksum-verified) and can be reused via
        `SPE_BSP_SRC_DIR`/`ARM_TOOLCHAIN_DIR` env overrides — sentinel
        ships the same artifacts at
        `scripts/spe/downloads/spe-freertos-bsp/`, so the two trees
        share a single download cache.

      The mod is registered alongside the other platform modules in the root
      `justfile` and in the `_orchestrate` loop, so `just setup` / `just
      doctor` cover SPE with everything else.

- [x] **100.8 — Mock-IVC E2E test**

      Landed `packages/testing/nros-tests/tests/orin_spe_mock_ivc.rs` as the
      executable spec for the IVC link wire format
      (`docs/roadmap/phase-100-04-link-ivc-design.md` §5). The test pins:

      - 64-byte fixed frame size, 4-byte LE `total_len + offset` header,
        60-byte max payload — matches the C `__z_ivc_send_batch` /
        `__z_ivc_recv_batch` state machines in
        `packages/zpico/zpico-sys/zenoh-pico/src/link/unicast/ivc.c`.
      - Single-frame round-trip, multi-frame reassembly (200 B → 4 frames),
        keep-alive (`total=0, offset=0`) drop, and protocol-violation
        rejection (offset != accumulated length on a fresh batch).

      Runs over the `nvidia-ivc/unix-mock` backend on a `SOCK_DGRAM` pair —
      no NVIDIA SDK or hardware needed. The CCPLEX-side bridge daemon in
      `autoware_sentinel/src/ivc-bridge/` cites this test as its byte-level
      conformance reference.

      A POSIX-side `examples/orin-spe/rust/zenoh/talker/` is **deferred** —
      a working binary needs (a) a POSIX FreeRTOS port crate (we don't ship
      one) and (b) `nros` umbrella propagation of a `link-ivc` feature plus
      a zpico-sys backend matrix that lets `orin-spe` co-exist with the
      `posix` UDP/TCP path. Both are larger lifts than fit this phase. The
      friend project's bridge daemon (autoware_sentinel Phase 11.6) will
      land the runnable end-to-end demo once it's the right side of those
      decisions.

      **Acceptance:** `cargo nextest run -p nros-tests --test orin_spe_mock_ivc`
      passes on Linux without any NVIDIA SDK. Wired into `just orin_spe test`
      alongside the `nvidia-ivc` `unix-mock` loopback (Phase 100.0).

## Acceptance criteria (phase-level)

- [x] All 10 sub-items above checked off (100.0, 100.0a, 100.1–100.8).
- [x] `cargo +nightly build --target armv7r-none-eabihf -Zbuild-std=core,alloc -p nros-platform-freertos --features cortex-r --no-default-features` succeeds with zero warnings. (Default `lwip` feature pulls broken socket types on `armv7r-none-eabihf`; the SPE consumer pulls `nros-platform-freertos` with `default-features = false` precisely to dodge that. `active` lives on the shim crates — not on `nros-platform-freertos` — so the original wording was a typo; corrected here.)
- [x] `cargo test -p nvidia-ivc --features unix-mock` loopback green on Linux.
- [x] POSIX-side mock IVC end-to-end test (`orin_spe_mock_ivc` in `nros-tests`) passes in `just test-all` against the `nvidia-ivc` `unix-mock` backend. Verified via `just orin_spe test` → 4/4 cases (`single_frame_message_round_trips`, `multi_frame_zenoh_batch_reassembles`, `keepalive_ping_is_dropped_silently`, `wire_violation_yields_protocol_error`).
- [x] `just orin_spe build` produces a `spe.bin` whose statically-linked size is reported (target `< 256 KB` but not gated — application-level fitting is `autoware_sentinel`'s job).
      - `just orin_spe bsp-download` fetches L4T 36.4.4 `public_sources.tbz2` + ARM GNU 13.2 toolchain into `external/spe-fsp/downloads/`.
      - `just orin_spe bsp-build` (with the path fix in `just/orin-spe.just`: `SRC` and `TC` are now `cd … && pwd`-resolved to absolute paths before the upstream Makefile invocation, since the FSP demo references `$(SPE_FREERTOS_BSP)/...` from its own working directory) builds `rt-aux-cpu-demo-fsp` and stages `external/spe-fsp/install/{lib,include}` (libtegra_aon_fsp.a 37 MB, libnewlib.a 1.3 MB, headers).
      - `just orin_spe build` then links the SPE board crate's rlib for `armv7r-none-eabihf` against the staged FSP. The upstream BSP's own demo `spe.bin` measures **136 KB** (`out/t23x/spe.bin`), well under the 256 KB BTCM budget — leaving ~120 KB headroom for the autoware_sentinel application layer (Phase 11.5).
- [x] `nros-rmw-zenoh` test suite passes both with and without `Z_FEATURE_LINK_IVC` enabled.

## Out of scope (handed off to autoware_sentinel)

- Reduced sentinel algorithm set selection (heartbeat watchdog vs. emergency-stop
  operator vs. cmd-gate trade-offs against the BTCM budget).
- Linux-side IVC bridge daemon (lives in `autoware_sentinel/src/ivc-bridge/`).
- SPE firmware flashing procedure (host USB recovery vs. UEFI capsule, see
  `autoware_sentinel` Phase 11.7).
- Hardware test on real Orin (also Phase 11.7).
- Float-ABI mismatch resolution at the application boundary (the board crate
  surfaces both options; the application picks).

## Risks

1. **NVIDIA FSP licensing.** The FSP ships under NVIDIA's SDK Manager EULA. The
   `nros-board-orin-spe` crate cannot vendor FSP sources; it can only `link =
   "tegra_aon_fsp"` against a user-supplied install. Document `NV_SPE_FSP_DIR` clearly.
   Anyone without an Orin DevKit account cannot build this board crate — same as the
   ESP32 pattern, where the Espressif fork is downloaded by the user post-clone.

2. **L4T 36.4 IVC on Orin not validated by us.** NVIDIA's earlier docs noted IVC was
   "verified only on AGX Xavier"; L4T 36.4 adds Orin support but unconfirmed by this
   project. Mitigation: 100.4–100.5 (POSIX mock) finishes first, hardware integration
   waits for `autoware_sentinel` Phase 11.4 to confirm IVC echo works on real Orin.

3. **256 KB BTCM is borderline.** zenoh-pico alone is 60–80 KB; FreeRTOS R5 port +
   FSP runtime is ~80 KB; nros-c + nros-node is ~30 KB. Application-side compromises
   (reduced sentinel set, no MPC) live in `autoware_sentinel`. nano-ros's job is to
   keep the **per-feature footprint reportable** — add `cargo bloat`-derived `.text`
   sizes per crate to the board crate's README, refreshed by CI.

4. **Float ABI mismatch.** NVIDIA BSP defaults to `-mfloat-abi=softfp`; the canonical
   Rust embedded target `armv7r-none-eabihf` is hard-float. If softfp is the only
   option (BSP doesn't recompile cleanly), switch Rust to `armv7r-none-eabi` (soft)
   and accept the perf hit.

5. **Cortex-M assumption leakage.** *Audited.* The `nros-platform-freertos`
   critical-section path is the only Rust-side leak and 100.1 fixed it; the M-side
   `cs_impl` is gated on `feature = "critical-section"`, the R-side on `feature =
   "cortex-r"`, and the lib raises a `compile_error!` if both are enabled. Other
   Cortex-M `cortex_m::` users (`nros-board-{stm32f4,mps2-an385}`,
   `nros-platform-{stm32f4,mps2-an385}`, `mps2-an385-pac`, `qemu-smoltcp-bridge`,
   `stm32f4-porting`) are board/platform-scoped to those Cortex-M targets and not
   pulled by `nros-board-orin-spe`. Found one C-side leak in
   `packages/core/nros-c/include/nros/platform/baremetal.h`: the
   `nros_platform_disable_irq` / `restore_irq` helpers gated on `__ARM_ARCH`
   (defined for *every* ARM profile) and emitted `mrs Rd, primask` —
   not assemblable on R/A profiles. Replaced with a profile-aware
   `__ARM_ARCH_PROFILE`-gated split: M uses PRIMASK, R/A uses CPSR I-bit
   (`mrs Rd, cpsr` / `msr cpsr_c, Rs`). Verified codegen for `cortex-m3` and
   `cortex-r5` produces correct mnemonics for each profile.

## Notes

- This phase deliberately **does not** introduce a new RMW backend. The Phase 90
  `nros-rmw-uorb` precedent shows what that costs (~500 LOC + new tests + new docs).
  Adding IVC as a zenoh-pico link transport is ~150 LOC of C in `ivc.c` plus the
  vtable shim — and gives us `rmw-zenoh` interoperability with Linux-side Autoware
  for free.
- Cortex-R52 (the dedicated Safety MCU on newer Orins) is not targeted by this
  phase. R52 differs from R5 in the GIC/interrupt path and the FSP variant — could
  be a future Phase 100.x sub-item, but the friend project's deployment target is
  the SPE R5F.
- The `external/freertos-kernel/portable/GCC/ARM_CR5/` GCC port and
  `external/freertos-kernel/portable/ThirdParty/GCC/Posix/` POSIX port are pulled
  into `external/` (gitignored) as reference. NVIDIA's FSP uses its own copy of the
  ARM_CR5 port with Tegra-specific tweaks; we don't replace it, only read it.

## Appendix A — IVC library landscape

IVC (Inter-VM Communication, but on AGX Orin used CCPLEX↔SPE) is a
NVIDIA-defined header-prefixed lock-free SPSC ring buffer in shared DRAM,
paired with an HSP (Hardware Synchronization Primitives) doorbell for wake.
Two sides — client + server — go through an asymmetric init handshake.
Frame size and frame count are fixed at carveout setup (typical: 16 frames
× 64 B per channel).

**It is not pub/sub.** No discovery, naming, QoS, or fanout. One channel =
one peer. That's why this phase puts IVC at the **link layer inside
zenoh-pico** (`Z_FEATURE_LINK_IVC`), peer to TCP/UDP/Serial/RawEth — not as
a new RMW backend.

### A.1 SPE / embedded side

| Source | License | Status | Notes |
|--------|---------|--------|-------|
| **NVIDIA FSP `tegra_ivc_channel_*`** | NVIDIA EULA (SDK Manager) | The only sanctioned path on SPE | Ships with the `tegra_aon_fsp` static lib in the FreeRTOS FSP. API: `tegra_ivc_channel_get(id)`, `tegra_ivc_channel_read/write`, `tegra_ivc_channel_notify`. Closed-source. **100.6 binds against this via `NV_SPE_FSP_DIR`.** |
| Linux kernel `drivers/firmware/tegra/ivc.c` + `include/soc/tegra/ivc.h` | GPL-2.0 | Upstream since v4.10 | Canonical open-source impl. ~500 LOC of C, mostly cache-coherent ring-buffer arithmetic. Reference for protocol semantics; **not** linked. Could be ported to `no_std` Rust as `nros-ivc-core` if the FSP becomes a portability blocker (out of scope for this phase). |
| `arm-trusted-firmware/drivers/nvidia/tegra/common/tegra_ivc.c` | BSD-3-Clause | In TF-A | Smaller boot-time init impl. Same protocol. Useful as a second open-source data point. |
| NVIDIA TLK / Trusty IVC | NVIDIA | Used in TLK secure-OS | Same protocol; not used here. |

### A.2 Linux host side (CCPLEX)

| Source | License | Status | Notes |
|--------|---------|--------|-------|
| **sysfs `/sys/devices/platform/bus@0/bus@0:aon_echo/data_channel`** | n/a | Shipped by L4T's `aon_echo` driver | Plain read/write file. **Simplest path; no library needed.** This is what `autoware_sentinel/src/ivc-bridge/` daemon uses (its Phase 11.6). |
| `/dev/tegra-ivc-N` chardev | n/a | Present in some L4T configs | Same data, ioctl-based. Not always available; sysfs is more universal. |
| L4T BSP `tegra_ivc_test` userspace tool | NVIDIA EULA | SDK Manager | ~150 LOC C wrapper around the sysfs node. Demo-grade. |
| `JetsonHacks/jetson-orin-aon-echo` (community) | various OSS | GitHub | Bash + small C wrappers around the sysfs node. Useful for quick verification on a fresh Orin. |

**No `libtegra-ivc` userspace library ships** — every Linux-side caller talks
to sysfs/chardev directly.

### A.3 Implications for this phase

1. **The "IVC library" is asymmetric, and we own one independent crate
   that captures both sides.** `packages/drivers/nvidia-ivc` (100.0) wraps
   the FSP on hardware and a Unix-socket pair in dev. SPE side gets the
   `fsp` feature; CCPLEX side (used in the `autoware_sentinel`
   `src/ivc-bridge/` daemon) can pull the same crate with `unix-mock` for
   testing or `std`-side sysfs read/write for production. One Rust API,
   two consumers.

2. **No portable IVC dep needed at the platform layer.** The
   `PlatformIvc` trait in `nros-platform-api` (100.0a) hides the driver
   choice; `nros-platform-orin-spe` (100.5) implements it via
   `nvidia-ivc`. zenoh-pico's `Z_FEATURE_LINK_IVC` C code calls the shim
   forwarders (100.3 + 100.4), which dispatch through `<P as PlatformIvc>`.
   Same chain pattern as `_z_open_tcp` → `<P as PlatformTcp>::open`.

3. **If we ever need an open-source IVC port** (e.g. running the link
   layer outside NVIDIA hardware, or supporting a non-Tegra SoC with a
   similar mailbox), the cleanest starting point is the GPL kernel impl —
   ~500 LOC, well-tested. Would land as a third backend feature
   (`nvidia-ivc/portable`) inside the same driver crate, no platform-layer
   churn.
