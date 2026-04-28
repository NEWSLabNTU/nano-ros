# Phase 100 — AGX Orin SPE infrastructure (Cortex-R5F + IVC)

**Goal:** ship the platform-level support nano-ros needs to run on NVIDIA Jetson AGX Orin's
**Sensor Processing Engine (SPE)** — a Cortex-R5F core running NVIDIA's FreeRTOS V10.4.3
FSP. Application-level work (porting the safety-island packages from the friend project
[`autoware_sentinel`](https://github.com/jerry73204/autoware_sentinel)) lives in that
repo's [Phase 11](https://github.com/jerry73204/autoware_sentinel/blob/main/docs/roadmap/11-orin-spe.md);
this phase delivers the pieces nano-ros has to provide to make Phase 11 buildable.

**Status:** Not Started.
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
│  zpico-platform-shim → zenoh-pico                          │
│      │                                                     │
│      └── Z_FEATURE_LINK_IVC (new) ── tegra_ivc_channel_*   │
│                                                            │
│  Application: autoware_sentinel reduced subset             │
└────────────────────────────────────────────────────────────┘
```

## Work items

- [ ] **100.1 — Cortex-R5 critical-section abstraction in `nros-platform-freertos`**

      Currently `packages/core/nros-platform-freertos` hard-codes Cortex-M PRIMASK for
      its critical-section primitives. Add a `cortex-r` feature that swaps the inline
      asm for ARMv7-R CPSR I-bit toggling (`mrs r0, cpsr; cpsid i; … msr cpsr_c, r0`),
      and gate the existing `cortex-m` path behind a `cortex-m` feature.

      **Files:**
      - `packages/core/nros-platform-freertos/src/lib.rs` (critical-section impl)
      - `packages/core/nros-platform-freertos/Cargo.toml` (feature flags)

      **Acceptance:** crate builds for both `thumbv7m-none-eabi` (existing MPS2 path) and
      `armv7r-none-eabihf` (new SPE path). No behavioural change on Cortex-M.

- [ ] **100.2 — `armv7r-none-eabihf` target in workspace toolchain wiring**

      The Rust target is Tier 3, supported by rustc since 1.49 but not pre-built.
      Requires `build-std` for `core`/`alloc` and a `rust-toolchain.toml`-pinned
      nightly. Wire into:

      - `tools/rust-toolchain.toml` (add `armv7r-none-eabihf` to `targets`)
      - `just/workspace.just` `rust-targets` recipe
      - `Cargo.toml` `[unstable]` build-std-features in the new board crate's
        `.cargo/config.toml`

      **Acceptance:** `cargo +nightly build --target armv7r-none-eabihf -Zbuild-std=core,alloc -p nros-platform-freertos` succeeds.

- [ ] **100.3 — `zpico-platform-shim` Cortex-R5 build**

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

- [ ] **100.4 — Vendored zenoh-pico: `Z_FEATURE_LINK_IVC`**

      Add IVC as a first-class link transport inside zenoh-pico. Modelled on the
      existing `serial` and `raweth` link families. Hooks behind `Z_FEATURE_LINK_IVC=1`
      (default off; enabled by zpico-sys when the SPE board crate selects it).

      **Files (new + edits in `packages/zpico/zpico-sys/zenoh-pico/`):**
      - `src/link/unicast/ivc.c` (new) — implements `_z_open_ivc`, `_z_listen_ivc`,
        `_z_close_ivc`, `_z_read_ivc`, `_z_send_ivc` against a transport-trait
        function-pointer table set by the platform (`tegra_ivc_channel_*` on hardware,
        Unix-domain-socket bridge in the POSIX simulator)
      - `include/zenoh-pico/link/config/ivc.h` (new) — `_z_endpoint_ivc_*` parsers,
        IVC-specific config keys (`channel_id`, `frame_size`)
      - `src/link/endpoint.c` — register `ivc/<N>` URI scheme
      - `src/link/link.c` — link-table entry, dispatch
      - `src/link/manager.c` — manager-side hookup
      - `include/zenoh-pico/link/link.h` — `Z_LINK_IVC` enum value + feature guards
      - `include/zenoh-pico/config.h` — `Z_FEATURE_LINK_IVC` default
      - `CMakeLists.txt` — conditional compile of `ivc.c`
      - `packages/zpico/zpico-sys/build.rs` — add IVC source list, propagate
        `cargo:rustc-cfg=feature="link_ivc"` when SPE/POSIX-mock is the target

      Key design constraint: zenoh messages routinely exceed the **64-byte IVC frame
      size**. The link layer owns reassembly with a length-prefixed framing protocol
      (header: u16 total length + u16 sequence; same protocol on both sides of the
      bridge, see 100.6). Re-uses the size-aware ring already present in zenoh-pico's
      buf abstractions.

      **Acceptance:**
      - `Z_FEATURE_LINK_IVC=0` default build is byte-identical to current.
      - `Z_FEATURE_LINK_IVC=1` builds on POSIX (uses Unix-socket transport) and on
        `armv7r-none-eabihf` (uses `tegra_ivc_channel_*` provided by the board crate).
      - Unit test in `tests/`: open two IVC endpoints in the same process via the
        Unix-socket backend, exchange a multi-frame message, assert reassembled bytes.

- [ ] **100.5 — Mock IVC transport (POSIX dev path)**

      Without this, every iteration on the IVC link logic would require flashing the
      SPE — a 5–10 minute round-trip via USB recovery (see Phase 11.7 in
      `autoware_sentinel`). The mock backend lets us run the same zenoh-pico IVC link
      against a Unix-domain-socket pair and a small bridge daemon (Phase 11.6 in
      `autoware_sentinel`) on a single Linux host.

      **Files:** the mock side is **all in zenoh-pico**: `_z_open_ivc` etc. take a
      vtable pointer. The real-hardware vtable (FSP `tegra_ivc_channel_*`) is provided
      by the SPE board crate (100.6); the mock vtable (Unix-socket I/O) is provided
      by `nros-platform-posix` behind a `link-ivc-mock` feature.

      **Acceptance:** `cargo test -p nros-rmw-zenoh --features link-ivc-mock` exchanges
      a string via the mock IVC vtable end-to-end.

- [ ] **100.6 — `nros-board-orin-spe` board crate**

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

- [ ] **100.7 — Justfile + setup wiring**

      Add `just/orin-spe.just` mod with the standard recipe set used by every other
      platform module: `setup`, `doctor`, `build`, `build-fixtures`, `test`, `clean`.

      - `setup` — verifies `armv7r-none-eabihf` target installed, FSP path env vars
        present, `arm-none-eabi-{gcc,ld,size}` on PATH; clones FSP samples to
        `external/nvidia-spe-fsp` if user has SDK Manager creds (best-effort).
      - `doctor` — read-only diagnostic.
      - `build` — invokes board crate build via `cargo +nightly build --target
        armv7r-none-eabihf -Zbuild-std=core,alloc`, then NVIDIA Makefile.
      - `flash` — `flash.sh -k A_spe-fw …` against an x86 host in USB recovery mode
        (same mechanism documented in `autoware_sentinel` Phase 11.7).

      Wire into the top-level `_orchestrate` loop in `justfile` so `just setup` /
      `just doctor` mention SPE alongside other platforms.

- [ ] **100.8 — Examples + smoke tests**

      Create one minimal end-to-end example that exercises the full stack on the
      POSIX simulator (no SPE hardware required for CI):

      - `examples/orin-spe/rust/zenoh/talker/` — publishes `std_msgs/Int32` on
        `/chatter` over `ivc/2`, runs as a FreeRTOS POSIX-port process.
      - `tests/orin-spe-mock-ivc.sh` — boots zenohd, the mock IVC bridge daemon (which
        actually lives in `autoware_sentinel/src/ivc-bridge/`, exec'd by the test
        harness), and the talker; subscribes from CLI; asserts message receipt.

      Hardware tests live in `autoware_sentinel` Phase 11.7 (require SPE flash);
      nano-ros CI only runs the POSIX path.

      **Acceptance:** `cargo nextest run -p nros-tests --test orin_spe_mock_ivc` passes
      on Linux without any NVIDIA SDK.

## Acceptance criteria (phase-level)

- [ ] All 8 sub-items above checked off.
- [ ] `cargo +nightly build --target armv7r-none-eabihf -Zbuild-std=core,alloc -p nros-platform-freertos --features cortex-r,active` succeeds with zero warnings.
- [ ] POSIX-side mock IVC end-to-end test (`orin_spe_mock_ivc`) passes in `just test-all`.
- [ ] `just orin_spe build` produces a `spe.bin` whose statically-linked size is reported (target `< 256 KB` but not gated — application-level fitting is `autoware_sentinel`'s job).
- [ ] `nros-rmw-zenoh` test suite passes both with and without `link-ivc-mock` feature.

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

5. **Cortex-M assumption leakage.** The `nros-platform-freertos` critical-section
   abstraction is the obvious cleanup, but other places may bake Cortex-M assumptions
   (interrupt numbering, SCB access, CMSIS macros). Run `grep -rn 'cortex_m::\|CMSIS\|SCB\|primask' packages/` after 100.1 lands and fix any leaks.

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

1. **The "IVC library" is asymmetric.** SPE side links against the closed
   NVIDIA FSP. Linux side opens a sysfs file. Phase 100 only needs the SPE
   binding; the Linux binding lives in `autoware_sentinel` Phase 11.6 and
   is just `read(2)`/`write(2)` on the sysfs node.

2. **No portable IVC dep needed.** zenoh-pico's `Z_FEATURE_LINK_IVC` (100.4)
   takes a vtable of function pointers. The board crate (100.6) populates
   it with FSP calls. The mock backend (100.5) populates it with Unix-socket
   I/O. Neither end of nano-ros pulls a third-party IVC dep.

3. **If we ever need an open-source IVC port** (e.g. to run the IVC link
   layer outside NVIDIA hardware, or to support a non-Tegra SoC with a
   similar mailbox), the cleanest starting point is the GPL kernel impl —
   ~500 LOC, well-tested. Track as a Phase 100.x sub-item only if a real
   second consumer materialises.
