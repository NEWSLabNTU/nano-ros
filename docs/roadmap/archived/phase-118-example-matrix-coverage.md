# Phase 118: Example Matrix Collapse Tracker

**Goal.** Collapse example source directories to:

```text
examples/<platform>/<language>/<case>/
```

The RMW becomes a build-time selection:

- Rust: `cargo build --no-default-features --features rmw-<rmw>`
- C/C++: `cmake -B build-<rmw> -S . -DNROS_RMW=<rmw>`
- Zephyr: `west build -- -DCONF_FILE="prj.conf;prj-<rmw>.conf"`

The retired shape is:

```text
examples/<platform>/<language>/<rmw>/<case>/
```

**Status.** In progress. Many collapsed case dirs already exist, but
legacy RMW-root dirs remain and the docs/tests still disagree about the
canonical shape.

**Absorbs.**

- Phase 167: NuttX Rust collapse/link regression.
- Phase 170: bare-metal Rust collapse.

Those are no longer separate ownership docs; their blockers are tracked
inline below.

**Done means.** For each checkbox, source has moved into the collapsed
`<platform>/<language>/<case>/` shape, build recipes select RMW by flag,
tests/fixtures know the new path, runtime E2E coverage has been rerun
for the touched standard cases, and the legacy `<rmw>/` dir is deleted
unless explicitly listed as a carve-out.

---

## Current Snapshot

Directory scan on 2026-05-22 still shows these RMW-root directories:

```text
px4/cpp/uorb
px4/rust/uorb
qemu-arm-nuttx/c/zenoh
qemu-arm-nuttx/cpp/zenoh
qemu-arm-nuttx/rust/zenoh
```

`px4/*/uorb` is a carve-out, not normal RMW-collapse debt. PX4 is
uORB-only and the canonical live example is the C++ module/check path.
`zephyr/cpp/cyclonedds/talker-aemv8r` is also a carve-out: it remains a
one-board Cyclone DDS reference while normal Zephyr C++ examples use the
collapsed `zephyr/cpp/<case>` layout with `prj-<rmw>.conf` overlays.

---

## Tracker

### 118.A — Native Host Examples

Native has collapsed case dirs, but legacy RMW-root source dirs still
exist. Collapse/remove them after parity is verified.

- [x] **118.A.1 — `examples/native/c/zenoh/`**
      Standard six cases and C-only variants (`custom-msg`,
      `custom-platform`, `custom-transport-loopback`, `logging`) are
      collapsed under `examples/native/c/<case>/`.
- [x] **118.A.2 — `examples/native/c/xrce/`**
      Fold XRCE C cases into `examples/native/c/<case>/` with
      `-DNROS_RMW=xrce`.
- [x] **118.A.3 — `examples/native/c/cyclonedds/`**
      Fold Cyclone C cases into `examples/native/c/<case>/` with
      `-DNROS_RMW=cyclonedds`; preserve idlc converter plumbing.
- [x] **118.A.4 — `examples/native/cpp/zenoh/`**
      Standard six cases and C++-only variants (`logging`, `parameters`)
      are collapsed under `examples/native/cpp/<case>/`.
- [x] **118.A.5 — `examples/native/cpp/cyclonedds/`**
      Fold Cyclone C++ cases into `examples/native/cpp/<case>/`.
- [x] **118.A.6 — `examples/native/rust/zenoh/`**
      Standard six cases and Zenoh-only variants (`*-rtic`,
      async service/action clients, custom transport, custom message,
      lifecycle, logging) are collapsed under
      `examples/native/rust/<case>/`.
- [x] **118.A.7 — `examples/native/rust/xrce/`**
      Standard six cases plus `serial-talker` and `serial-listener` are
      folded into `examples/native/rust/<case>/`; serial examples remain
      XRCE-only by feature/config, not by directory depth.
- [x] **118.A.8 — Native Rust Cyclone service runtime blocker**
      Closed by 171.C.1: native Rust Cyclone service server/client
      round-trip passed 4/4 after backend stale-pending cleanup.

### 118.B — ThreadX Linux Host Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots remain.
Cyclone C/C++ fixture support exists when local Cyclone artifacts are
installed; Rust Cyclone still depends on Phase 175-style staticlib work.

- [x] **118.B.1 — `examples/threadx-linux/c/zenoh/`**
      Closed 2026-05-21: fixture builders and `just threadx_linux`
      recipes use `examples/threadx-linux/c/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.B.2 — `examples/threadx-linux/cpp/zenoh/`**
      Closed 2026-05-21: fixture builders and `just threadx_linux`
      recipes use `examples/threadx-linux/cpp/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.B.3 — `examples/threadx-linux/rust/zenoh/`**
      Closed 2026-05-21: `just threadx_linux build-examples`,
      `build-fixtures`, run helpers, and tests use
      `examples/threadx-linux/rust/<case>/` with
      `--features rmw-zenoh --target-dir target-zenoh`; the legacy Zenoh
      root was deleted.
- [x] **118.B.4 — ThreadX Linux Cyclone Rust path**
      Explicitly deferred to Phase 175: pure Cargo ThreadX Linux Rust
      cannot link the C++ Cyclone backend directly until Cyclone staticlib
      support is available for non-CMake Rust examples.

### 118.C — ThreadX RISC-V QEMU Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots were
removed in 118.C; RMW selection now flows through `-DNROS_RMW=zenoh`
for C/C++ and `--features rmw-zenoh --target-dir target-zenoh` for Rust.

- [x] **118.C.1 — `examples/qemu-riscv64-threadx/c/zenoh/`**
      Deleted after collapsed `examples/qemu-riscv64-threadx/c/<case>/`
      CMake fixtures were wired to `-DNROS_RMW=zenoh` and
      `build-zenoh/`.
- [x] **118.C.2 — `examples/qemu-riscv64-threadx/cpp/zenoh/`**
      Deleted after collapsed `examples/qemu-riscv64-threadx/cpp/<case>/`
      CMake fixtures were wired to `-DNROS_RMW=zenoh` and
      `build-zenoh/`.
- [x] **118.C.3 — `examples/qemu-riscv64-threadx/rust/zenoh/`**
      Deleted after Rust recipes and fixture resolvers moved to
      `examples/qemu-riscv64-threadx/rust/<case>/` with `rmw-zenoh`
      and `target-zenoh/`.
- [x] **118.C.4 — ThreadX RISC-V Cyclone availability decision**
      Deferred. Cyclone DDS over this target needs the same hosted
      NetX-Duo BSD/socket integration as the wider Phase 175
      Cyclone RTOS gate; the migrated Rust `rmw-cyclonedds` feature
      remains defined but is not built in the 118.C fixture tier.

### 118.D — FreeRTOS QEMU Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots remain.
Cyclone on FreeRTOS is intentionally gated on an upstream-scale Cyclone
DDS RTOS/socket port.

- [x] **118.D.1 — `examples/qemu-arm-freertos/c/zenoh/`**
      Closed 2026-05-21: fixture builders and `just freertos`
      recipes use `examples/qemu-arm-freertos/c/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.D.2 — `examples/qemu-arm-freertos/cpp/zenoh/`**
      Closed 2026-05-21: fixture builders and `just freertos`
      recipes use `examples/qemu-arm-freertos/cpp/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.D.3 — `examples/qemu-arm-freertos/rust/zenoh/`**
      Closed 2026-05-21: `just freertos build-examples`,
      `build-fixtures`, run helpers, trace helpers, and tests use
      `examples/qemu-arm-freertos/rust/<case>/` with
      `--features rmw-zenoh --target-dir target-zenoh`; the legacy
      Zenoh root was deleted.
- [x] **118.D.4 — FreeRTOS Cyclone gate recorded**
      Won't fit until Cyclone DDS gains the required FreeRTOS/lwIP hosted
      runtime layer.

### 118.E — NuttX QEMU Examples

Absorbs Phase 167. NuttX now uses collapsed case dirs for C, C++, and
Rust. Zenoh is the only live NuttX RMW; Cyclone stays gated below.

- [x] **118.E.1 — `examples/qemu-arm-nuttx/c/zenoh/`**
      Closed 2026-05-22: fixture builders and tests use
      `examples/qemu-arm-nuttx/c/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.E.2 — `examples/qemu-arm-nuttx/cpp/zenoh/`**
      Closed 2026-05-22: fixture builders and tests use
      `examples/qemu-arm-nuttx/cpp/<case>/` with
      `-DNROS_RMW=zenoh`; the legacy Zenoh root was deleted.
- [x] **118.E.3 — `examples/qemu-arm-nuttx/rust/zenoh/`**
      Closed 2026-05-22: Rust Zenoh cases moved to
      `examples/qemu-arm-nuttx/rust/<case>/`; just recipes and fixture
      resolvers now use the collapsed paths.
- [x] **118.E.4 — NuttX Rust collapsed-shape link regression**
      Closed 2026-05-22: the collapsed Rust crates link after adjusting
      their local dependency and `[patch.crates-io]` paths from the
      former depth-5 layout to the depth-4 layout.
- [x] **118.E.5 — NuttX Cyclone gate recorded**
      Cyclone on NuttX is deferred behind a hosted NuttX socket/runtime
      port for Cyclone DDS.

### 118.F — Zephyr Examples

Zephyr mostly uses collapsed dirs with `prj-<rmw>.conf` overlays.
Remaining RMW-root dirs are legacy or special-case.

- [x] **118.F.1 — `examples/zephyr/rust/xrce/`**
      Superseded by `examples/zephyr/rust/<case>/` plus
      `prj-xrce.conf`; only ignored generated leftovers remained and were
      removed.
- [x] **118.F.2 — `examples/zephyr/rust/dds/`**
      Superseded by `examples/zephyr/rust/<case>/` plus
      `prj-cyclonedds.conf`. After Phase 169, DDS means Cyclone; no
      dust-DDS Zephyr Rust paths remain in the live tree.
- [x] **118.F.3 — `examples/zephyr/cpp/cyclonedds/`**
      `talker-aemv8r` remains a documented one-board Cyclone DDS
      carve-out for the FVP/AEMv8R path; normal C++ cases stay collapsed
      under `examples/zephyr/cpp/<case>/`.
- [x] **118.F.4 — Zephyr C collapsed dirs**
      Current live C Zephyr examples are under `examples/zephyr/c/<case>/`.
- [x] **118.F.5 — Zephyr C++ collapsed dirs**
      Current live C++ Zephyr examples are under `examples/zephyr/cpp/<case>/`.
- [x] **118.F.6 — Zephyr Rust collapsed dirs**
      Current live Rust Zephyr examples are under `examples/zephyr/rust/<case>/`.

### 118.G — Bare-Metal Rust Examples

Absorbs Phase 170. These targets have board-specific feature gates, so
collapse is per-board rather than mechanical.

#### 118.G Known Runtime Follow-Ups

The 118.G directory collapse is complete, but the post-collapse E2E rerun
found runtime transport failures that remain open. These are tracked here
instead of reopening the source-layout checkboxes, because the moved
examples build and the fixture resolver now finds the collapsed paths.

- [ ] **118.G.runtime.1 — QEMU MPS2 serial Zenoh pub/sub**
      `test_qemu_serial_pubsub_e2e` failed 5/5 focused retries on
      2026-05-22. Both firmware images boot, the listener subscribes,
      and the talker reaches `Publishing messages over serial...`, but no
      `Published:` line appears and the test ends with
      `published=0, received=0`. Suspect transport progress in the
      serial/zenoh-pico path rather than stale fixture paths.
- [ ] **118.G.runtime.2 — ESP32-C3 QEMU OpenETH Zenoh pub/sub**
      The ESP32 E2E group failed 3/3 focused retries on 2026-05-22:
      `test_esp32_talker_listener_e2e`, `test_esp32_to_native`, and
      `test_native_to_esp32` all moved zero messages. Listener firmware
      boots and subscribes; diagnostic counters show `do_poll` increasing
      while `cb_hits=0`, `bridge_polls=0`, and `tx_drained=0`, which
      points at the smoltcp poll callback not being reached from the
      active zenoh-pico/nros-smoltcp path.
- [x] **118.G.runtime.3 — XRCE large-message E2E setup gate**
      Initial `just qemu test-all` reported four XRCE large-message
      failures because `MicroXRCEAgent` was missing. Running
      `just xrce setup` built `build/xrce-agent/MicroXRCEAgent`, and the
      focused XRCE large-message subset then passed 4/4.

- [x] **118.G.1 — qemu-arm bare-metal Zenoh collapse**
      Closed 2026-05-22: `talker`, `listener`, serial, RTIC, and mixed RTIC
      cases live under `examples/qemu-arm-baremetal/rust/<case>/`; workspace
      members, `just qemu` run helpers, and fixture resolvers use the
      collapsed paths.
- [x] **118.G.2 — qemu-arm bare-metal DDS legacy decision**
      Closed 2026-05-22: dust-DDS was already retired and no qemu-arm
      bare-metal DDS source directory remains. No Cyclone replacement is
      tracked here because Cyclone requires hosted sockets, threads, heap,
      and libc.
- [x] **118.G.3 — qemu-esp32 bare-metal Zenoh collapse**
      Closed 2026-05-22: `talker` and `listener` live under
      `examples/qemu-esp32-baremetal/rust/<case>/`; `just esp32 build-qemu`
      and ESP32 fixture builders use the collapsed paths.
- [x] **118.G.4 — qemu-esp32 bare-metal DDS legacy decision**
      Closed 2026-05-22: the dust-DDS qemu-esp32 bare-metal dirs are absent;
      no Cyclone replacement is expected for the pure bare-metal target.
- [x] **118.G.5 — real ESP32 Zenoh collapse**
      Closed 2026-05-22: real ESP32 `talker` and `listener` live under
      `examples/esp32/rust/<case>/`; `just esp32 build-examples` uses the
      collapsed paths.
- [x] **118.G.6 — STM32F4 Zenoh collapse**
      Closed 2026-05-22: STM32F4 `talker`, RTIC service/action cases, and
      `talker-embassy` live under `examples/stm32f4/rust/<case>/`; fixture
      recipes and binary resolvers use the collapsed paths.
- [x] **118.G.7 — Bare-metal C/C++ empty cells documented**
      No C/C++ bare-metal harness is expected in this phase.
- [x] **118.G.8 — Bare-metal Cyclone gate recorded**
      Cyclone DDS requires BSD sockets, threads, heap, and libc; pure
      bare-metal cells use Zenoh/XRCE-class embedded backends instead.

### 118.H — PX4 / uORB Carve-Outs

PX4 is not a normal RMW matrix cell.

- [x] **118.H.1 — `examples/px4/cpp/uorb/` carve-out**
      PX4 uses uORB, and the live surface is C++.
- [x] **118.H.2 — `examples/px4/rust/uorb/` README-only placeholder**
      Historical Rust uORB path retained as documentation only unless a
      future uORB Rust backend returns.

### 118.I — Docs, Recipes, and Lint

- [x] **118.I.1 — `examples/README.md` canonical shape**
      Rewrite stale README text that still calls
      `<platform>/<language>/<rmw>/<case>` canonical.
- [x] **118.I.2 — `CLAUDE.md` / AGENTS consistency**
      Keep the canonical shape in memory files aligned with this tracker.
- [x] **118.I.3 — Just recipes**
      Build fixtures from collapsed dirs and pass RMW by feature/CMake arg
      instead of walking legacy RMW roots.
- [x] **118.I.4 — Test fixture paths**
      Remove remaining pre-collapse fixture paths from
      `packages/testing/nros-tests`.
- [x] **118.I.5 — Matrix lint**
      Add a script/test that fails on new untriaged
      `<platform>/<language>/<rmw>/` roots.
- [x] **118.I.6 — Archive absorbed docs**
      After this tracker is accepted, move Phase 167 and Phase 170 to
      `docs/roadmap/archived/` or leave short stubs pointing here.

---

## Implementation Notes

### Rust Shape

Each collapsed Rust case owns optional RMW deps and mutually exclusive
features:

```toml
[features]
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh"]
rmw-xrce = ["dep:nros-rmw-xrce-cffi"]
rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys"]
```

Cyclone Rust is special: `nros-rmw-cyclonedds-sys` exposes the C
registration shim, but pure Cargo cannot build/link the C++ Cyclone DDS
backend and idlc descriptors. Native Rust Cyclone examples currently use
CMake/Corrosion staticlib entry points; embedded Cyclone Rust remains
owned by the Phase 175-class build path.

### C / C++ Shape

Each collapsed C/C++ case should configure with:

```cmake
set(NANO_ROS_RMW "${NROS_RMW}" CACHE STRING "zenoh|xrce|cyclonedds")
```

and build in isolated dirs:

```text
build-zenoh/
build-xrce/
build-cyclonedds/
```

Runtime tests must consume these prebuilt fixture binaries. They should
not configure or compile examples inside nextest test bodies; missing
fixtures should report a `[SKIPPED]` prerequisite and point back to
`just <platform> build-fixtures`.

### Zephyr Shape

Zephyr keeps one source dir per case and selects RMW via overlays:

```text
prj.conf
prj-zenoh.conf
prj-xrce.conf
prj-cyclonedds.conf
```

Legacy `zephyr/<lang>/<rmw>/<case>/` dirs should disappear except for
explicitly documented one-board reference cases.

---

## Acceptance Criteria

- [x] No untriaged `examples/<platform>/<language>/<rmw>/` roots remain.
      The remaining third-axis directories are documented carve-outs:
      `examples/px4/{cpp,rust}/uorb` and
      `examples/zephyr/cpp/cyclonedds/talker-aemv8r`.
- [x] Every Rust collapsed case builds for each `rmw-*` feature it exposes
      with isolated `target-<rmw>/`, except deliberately deferred Cyclone
      embedded cells recorded under 118.B.4, 118.C.4, 118.D.4, 118.E.5,
      and 118.G.8.
- [x] Every C/C++ collapsed case configures for each supported RMW with
      isolated `build-<rmw>/`; unsupported embedded Cyclone cells are
      recorded as phase gates instead of exposed build targets.
- [x] Runtime E2E tests were rerun after path-collapse edits, not only
      build/path smokes. The rerun closed the XRCE large-message setup
      gate and left two runtime follow-ups open: 118.G.runtime.1
      (QEMU MPS2 serial Zenoh pub/sub) and 118.G.runtime.2
      (ESP32-C3 QEMU OpenETH Zenoh pub/sub).
- [x] Zephyr collapsed cases select RMW through overlays, not source-dir
      duplication; the remaining C++ Cyclone DDS AEMv8R path is a
      documented one-board reference carve-out.
- [x] `examples/README.md` and memory docs agree on the canonical shape.
- [x] Test fixture builders use collapsed dirs only, except documented
      carve-outs.
- [x] A matrix lint prevents reintroducing the retired directory axis.
