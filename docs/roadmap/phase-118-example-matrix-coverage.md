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
tests/fixtures know the new path, and the legacy `<rmw>/` dir is deleted
unless explicitly listed as a carve-out.

---

## Current Snapshot

Directory scan on 2026-05-21 still shows these RMW-root directories:

```text
esp32/rust/zenoh
native/c/cyclonedds
native/c/xrce
native/c/zenoh
native/cpp/cyclonedds
native/cpp/zenoh
native/rust/xrce
native/rust/zenoh
px4/cpp/uorb
px4/rust/uorb
qemu-arm-baremetal/rust/zenoh
qemu-arm-freertos/c/zenoh
qemu-arm-freertos/cpp/zenoh
qemu-arm-freertos/rust/zenoh
qemu-arm-nuttx/c/zenoh
qemu-arm-nuttx/cpp/zenoh
qemu-arm-nuttx/rust/zenoh
qemu-esp32-baremetal/rust/dds
qemu-esp32-baremetal/rust/zenoh
qemu-riscv64-threadx/c/zenoh
qemu-riscv64-threadx/cpp/zenoh
qemu-riscv64-threadx/rust/zenoh
stm32f4/rust/zenoh
threadx-linux/c/zenoh
threadx-linux/cpp/zenoh
threadx-linux/rust/zenoh
zephyr/cpp/cyclonedds
zephyr/rust/dds
zephyr/rust/xrce
```

`px4/*/uorb` is a carve-out, not normal RMW-collapse debt. PX4 is
uORB-only and the canonical live example is the C++ module/check path.

---

## Tracker

### 118.A — Native Host Examples

Native has collapsed case dirs, but legacy RMW-root source dirs still
exist. Collapse/remove them after parity is verified.

- [ ] **118.A.1 — `examples/native/c/zenoh/`**
      Collapse remaining Zenoh C-only cases into `examples/native/c/<case>/`
      or delete when superseded by the collapsed dirs.
- [ ] **118.A.2 — `examples/native/c/xrce/`**
      Fold XRCE C cases into `examples/native/c/<case>/` with
      `-DNROS_RMW=xrce`.
- [ ] **118.A.3 — `examples/native/c/cyclonedds/`**
      Fold Cyclone C cases into `examples/native/c/<case>/` with
      `-DNROS_RMW=cyclonedds`; preserve idlc converter plumbing.
- [ ] **118.A.4 — `examples/native/cpp/zenoh/`**
      Fold Zenoh C++ cases into `examples/native/cpp/<case>/`.
- [ ] **118.A.5 — `examples/native/cpp/cyclonedds/`**
      Fold Cyclone C++ cases into `examples/native/cpp/<case>/`.
- [ ] **118.A.6 — `examples/native/rust/zenoh/`**
      Fold remaining Zenoh-only Rust variants/cases into
      `examples/native/rust/<case>/` where they are canonical cases.
- [ ] **118.A.7 — `examples/native/rust/xrce/`**
      Fold XRCE Rust cases into `examples/native/rust/<case>/` with
      `rmw-xrce`.
- [x] **118.A.8 — Native Rust Cyclone service runtime blocker**
      Closed by 171.C.1: native Rust Cyclone service server/client
      round-trip passed 4/4 after backend stale-pending cleanup.

### 118.B — ThreadX Linux Host Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots remain.
Cyclone C/C++ fixture support exists when local Cyclone artifacts are
installed; Rust Cyclone still depends on Phase 175-style staticlib work.

- [ ] **118.B.1 — `examples/threadx-linux/c/zenoh/`**
      Delete after `examples/threadx-linux/c/<case>/ -DNROS_RMW=zenoh`
      fixture parity is confirmed.
- [ ] **118.B.2 — `examples/threadx-linux/cpp/zenoh/`**
      Delete after collapsed C++ Zenoh fixture parity is confirmed.
- [ ] **118.B.3 — `examples/threadx-linux/rust/zenoh/`**
      Delete after collapsed Rust Zenoh fixture parity is confirmed.
- [ ] **118.B.4 — ThreadX Linux Cyclone Rust path**
      Add or explicitly defer the Rust Cyclone build path; pure cargo
      cannot link the C++ Cyclone backend directly.

### 118.C — ThreadX RISC-V QEMU Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots remain.

- [ ] **118.C.1 — `examples/qemu-riscv64-threadx/c/zenoh/`**
- [ ] **118.C.2 — `examples/qemu-riscv64-threadx/cpp/zenoh/`**
- [ ] **118.C.3 — `examples/qemu-riscv64-threadx/rust/zenoh/`**
- [ ] **118.C.4 — ThreadX RISC-V Cyclone availability decision**
      Document whether Cyclone over NetX-Duo BSD shim is in scope for this
      target or explicitly deferred.

### 118.D — FreeRTOS QEMU Examples

Collapsed case dirs exist for C/C++/Rust. Legacy Zenoh roots remain.
Cyclone on FreeRTOS is intentionally gated on an upstream-scale Cyclone
DDS RTOS/socket port.

- [ ] **118.D.1 — `examples/qemu-arm-freertos/c/zenoh/`**
- [ ] **118.D.2 — `examples/qemu-arm-freertos/cpp/zenoh/`**
- [ ] **118.D.3 — `examples/qemu-arm-freertos/rust/zenoh/`**
- [x] **118.D.4 — FreeRTOS Cyclone gate recorded**
      Won't fit until Cyclone DDS gains the required FreeRTOS/lwIP hosted
      runtime layer.

### 118.E — NuttX QEMU Examples

Absorbs Phase 167. C/C++ collapsed case dirs exist; Rust remains legacy
because the depth-4 collapsed shape hit a `build-std`/newlib link
regression.

- [ ] **118.E.1 — `examples/qemu-arm-nuttx/c/zenoh/`**
      Delete after collapsed C Zenoh fixture parity is confirmed.
- [ ] **118.E.2 — `examples/qemu-arm-nuttx/cpp/zenoh/`**
      Delete after collapsed C++ Zenoh fixture parity is confirmed.
- [ ] **118.E.3 — `examples/qemu-arm-nuttx/rust/zenoh/`**
      Collapse Rust Zenoh cases to `examples/qemu-arm-nuttx/rust/<case>/`
      after 118.E.4 is fixed.
- [ ] **118.E.4 — NuttX Rust collapsed-shape link regression**
      Fix the absorbed Phase 167 blocker: the depth-4 Rust layout fails
      with `undefined reference to __libc_init_array` /
      `__libc_fini_array`, while the depth-5 legacy
      `rust/zenoh/<case>` layout links. Investigate `build-std` libc
      patch scope, newlib/libgloss startup selection, and emitted
      `-nostartfiles` / `-nodefaultlibs`.
- [x] **118.E.5 — NuttX Cyclone gate recorded**
      Cyclone on NuttX is deferred behind a hosted NuttX socket/runtime
      port for Cyclone DDS.

### 118.F — Zephyr Examples

Zephyr mostly uses collapsed dirs with `prj-<rmw>.conf` overlays.
Remaining RMW-root dirs are legacy or special-case.

- [ ] **118.F.1 — `examples/zephyr/rust/xrce/`**
      Fold XRCE Rust cases into `examples/zephyr/rust/<case>/` or delete
      if superseded by the collapsed overlay dirs.
- [ ] **118.F.2 — `examples/zephyr/rust/dds/`**
      Retire or migrate legacy DDS Rust dirs. After Phase 169, the DDS
      backend is Cyclone; do not recreate dust-DDS paths.
- [ ] **118.F.3 — `examples/zephyr/cpp/cyclonedds/`**
      Decide whether `talker-aemv8r` remains a documented
      one-board/one-RMW reference carve-out or gets folded into the
      collapsed C++ talker overlays.
- [x] **118.F.4 — Zephyr C collapsed dirs**
      Current live C Zephyr examples are under `examples/zephyr/c/<case>/`.
- [x] **118.F.5 — Zephyr C++ collapsed dirs**
      Current live C++ Zephyr examples are under `examples/zephyr/cpp/<case>/`.
- [x] **118.F.6 — Zephyr Rust collapsed dirs**
      Current live Rust Zephyr examples are under `examples/zephyr/rust/<case>/`.

### 118.G — Bare-Metal Rust Examples

Absorbs Phase 170. These targets have board-specific feature gates, so
collapse is per-board rather than mechanical.

- [ ] **118.G.1 — `examples/qemu-arm-baremetal/rust/zenoh/`**
      Collapse `talker`, `listener`, and RTIC variants that are canonical
      standalone cases. Keep variant suffixes such as `talker-rtic`.
- [ ] **118.G.2 — qemu-arm bare-metal DDS legacy decision**
      Dust-DDS is retired; either remove old DDS cells if absent/stale or
      document no Cyclone replacement because Cyclone requires a hosted
      runtime.
- [ ] **118.G.3 — `examples/qemu-esp32-baremetal/rust/zenoh/`**
      Collapse `talker` and `listener` to `rust/<case>/`.
- [ ] **118.G.4 — `examples/qemu-esp32-baremetal/rust/dds/`**
      Retire dust-DDS dirs or replace with documented no-Cyclone decision.
- [ ] **118.G.5 — `examples/esp32/rust/zenoh/`**
      Collapse real ESP32 Zenoh `talker` and `listener` to `rust/<case>/`.
- [ ] **118.G.6 — `examples/stm32f4/rust/zenoh/`**
      Collapse Zenoh cases to `rust/<case>/`; keep RTIC/Embassy variants as
      suffix-named cases.
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

- [ ] **118.I.1 — `examples/README.md` canonical shape**
      Rewrite stale README text that still calls
      `<platform>/<language>/<rmw>/<case>` canonical.
- [ ] **118.I.2 — `CLAUDE.md` / AGENTS consistency**
      Keep the canonical shape in memory files aligned with this tracker.
- [ ] **118.I.3 — Just recipes**
      Build fixtures from collapsed dirs and pass RMW by feature/CMake arg
      instead of walking legacy RMW roots.
- [ ] **118.I.4 — Test fixture paths**
      Remove remaining pre-collapse fixture paths from
      `packages/testing/nros-tests`.
- [ ] **118.I.5 — Matrix lint**
      Add a script/test that fails on new untriaged
      `<platform>/<language>/<rmw>/` roots.
- [ ] **118.I.6 — Archive absorbed docs**
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

- [ ] No untriaged `examples/<platform>/<language>/<rmw>/` roots remain.
- [ ] Every Rust collapsed case builds for each `rmw-*` feature it exposes
      with isolated `target-<rmw>/`.
- [ ] Every C/C++ collapsed case configures for each supported RMW with
      isolated `build-<rmw>/`.
- [ ] Zephyr collapsed cases select RMW through overlays, not source-dir
      duplication.
- [ ] `examples/README.md` and memory docs agree on the canonical shape.
- [ ] Test fixture builders use collapsed dirs only, except documented
      carve-outs.
- [ ] A matrix lint prevents reintroducing the retired directory axis.
