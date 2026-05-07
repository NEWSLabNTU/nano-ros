# Phase 117: Example Matrix Coverage Completion

**Goal:** Close the (platform × language × RMW × case) example-coverage matrix to a documented, intentional shape — every supported cell either ships an example set or is explicitly marked "out of scope" with a one-line reason.

**Status:** Not Started
**Priority:** Medium
**Depends on:** Phase 78 (`colcon-nano-ros` for cross-language codegen), Phase 112 (C/C++ API ergonomics), Phase 114 (sample-yaml landing per example), Phase 115 (custom-transport surface unblocks transport-light demos)
**Related:** `examples/README.md`, CLAUDE.md "Examples = Standalone Projects" section

---

## Overview

The example tree is the project's most-touched documentation. Today it grew organically per phase, leaving asymmetric coverage:

- **XRCE-DDS** has full C/C++/Rust examples on `native` + `zephyr` only — six other RTOS targets that already run zenoh + dust-DDS have no XRCE example, even though `nros-rmw-xrce` works there.
- **DDS over C/C++** ships only on `native` + `zephyr`; every other RTOS has Rust-only DDS examples while the same dust-DDS C ABI is reachable.
- **Service-/action-client + server** variants are universal in `native/*` and `zephyr/*`, partial on `qemu-arm-{freertos,nuttx}/*` zenoh, and absent on every other Rust-on-RTOS cell.
- **C/C++ on bare-metal-Rust platforms** (`esp32`, `qemu-arm-baremetal`, `qemu-esp32-baremetal`, `stm32f4`) is structurally absent — there is no C harness on those targets. This is the one cell-set that is genuinely blocked, not an oversight.

This phase produces a single source of truth for the matrix, fills the achievable gaps, and writes the rationale for the deliberate holes so future phases inherit the decision.

---

## Architecture

### A. The matrix

Three axes:

| Axis | Values |
|------|--------|
| **Platform** | `native`, `esp32`, `px4`, `qemu-arm-baremetal`, `qemu-arm-freertos`, `qemu-arm-nuttx`, `qemu-esp32-baremetal`, `qemu-riscv64-threadx`, `stm32f4`, `threadx-linux`, `zephyr` |
| **Language** | `c`, `cpp`, `rust` |
| **RMW**      | `zenoh`, `dds`, `xrce` |

Cases per cell: `talker`, `listener`, `service-client`, `service-server`, `action-client`, `action-server`. Special cases (`custom-msg`, `serial-talker`, `large-msg-test`, `lifecycle-node`, `rtic-*`, `async-*`, `stress-test`, `fairness-bench`, `wcet-bench`, `cdr-test`, `baremetal-demo`) live alongside but are out-of-scope for the matrix; they document themselves.

### B. Current state (snapshot)

Cell existence (Y = at least one example, `-` = none):

```
platform                 lang  zenoh  dds    xrce
native                   c     Y      Y      Y
native                   cpp   Y      Y      -
native                   rust  Y      Y      Y
esp32                    c     -      -      -
esp32                    cpp   -      -      -
esp32                    rust  Y      -      -
px4                      c     -      -      -
px4                      cpp   -      -      -
px4                      rust  -      -      -    (uorb only)
qemu-arm-baremetal       c     -      -      -
qemu-arm-baremetal       cpp   -      -      -
qemu-arm-baremetal       rust  Y      Y      -
qemu-arm-freertos        c     Y      -      -
qemu-arm-freertos        cpp   Y      -      -
qemu-arm-freertos        rust  Y      Y      -
qemu-arm-nuttx           c     Y      -      -
qemu-arm-nuttx           cpp   Y      -      -
qemu-arm-nuttx           rust  Y      Y      -
qemu-esp32-baremetal     c     -      -      -
qemu-esp32-baremetal     cpp   -      -      -
qemu-esp32-baremetal     rust  Y      Y      -
qemu-riscv64-threadx     c     Y      -      -
qemu-riscv64-threadx     cpp   Y      -      -
qemu-riscv64-threadx     rust  Y      Y      -
stm32f4                  c     -      -      -
stm32f4                  cpp   -      -      -
stm32f4                  rust  Y      -      -
threadx-linux            c     Y      -      -
threadx-linux            cpp   Y      -      -
threadx-linux            rust  Y      Y      -
zephyr                   c     Y      Y      Y
zephyr                   cpp   Y      Y      Y
zephyr                   rust  Y      Y      Y
```

### C. Gap classification

Three buckets:

1. **Achievable today (mechanical port)** — language toolchain + RMW backend already work on the platform; missing examples are ports of the canonical `talker/listener/service-*/action-*` set from a sibling cell.
2. **Achievable but transport-dependent** — XRCE on most RTOS targets needs a UDP/serial agent reachable from QEMU. The zenoh examples already prove the network plumbing works; XRCE inherits it via `nros-rmw-xrce`.
3. **Out of scope** — bare-metal C/C++ has no harness (no nros-c bare-metal startup/linker), and `px4` is uORB-only by construction. These are documented holes.

### D. Coverage policy

After Phase 117 lands, the matrix has exactly two states per cell:

- **Filled** with the canonical six cases, OR
- **Documented out-of-scope** in `examples/README.md` with a one-line reason and the gating phase (e.g. "qemu-arm-baremetal/c: blocked on Phase 9999 bare-metal C harness").

A `just check-example-matrix` lint walks the tree and the README and fails on any cell that is neither filled nor documented.

---

## Work Items

### Tier 1 — Inventory + policy

- [ ] **117.A.1 — Matrix snapshot script.** `tools/example-matrix.py` walks `examples/` and prints the table from §B in the same shape, plus per-cell case completeness. Lives next to `tools/`'s existing scripts. Used by 117.A.2 and the eventual lint.
- [ ] **117.A.2 — `examples/README.md` matrix table.** Replace the current ad-hoc list with the autogenerated table, plus a "Deliberately empty" subsection that names every out-of-scope cell with its reason.
- [ ] **117.A.3 — `just check-example-matrix` lint.** Wraps `tools/example-matrix.py --lint`. Wired into `just ci` and PR CI.

### Tier 2 — Achievable cells (canonical six cases)

Each item ships `talker, listener, service-{client,server}, action-{client,server}` for the named cell. Reuse the existing sibling cell as the template (most patches are board-config + Cargo manifest tweaks).

#### XRCE-DDS coverage

- [ ] **117.B.1 — `qemu-arm-freertos/{c,cpp,rust}/xrce/`.** Template: `zephyr/c/xrce/`. Transport: UDP via slirp + MicroXRCEAgent on host. Six cases each language → 18 examples.
- [ ] **117.B.2 — `qemu-arm-nuttx/{c,cpp,rust}/xrce/`.** Template: `zephyr/*/xrce/`. NuttX UDP path same as zenoh. 18 examples.
- [ ] **117.B.3 — `qemu-riscv64-threadx/{c,cpp,rust}/xrce/`.** ThreadX NetX-Duo BSD socket layer is the agent transport. 18 examples.
- [ ] **117.B.4 — `threadx-linux/{c,cpp,rust}/xrce/`.** Bridge-net path; mirrors threadx-linux zenoh. 18 examples.
- [ ] **117.B.5 — `qemu-esp32-baremetal/rust/xrce/`.** ESP32 LWIP UDP. Six cases.
- [ ] **117.B.6 — `esp32/rust/xrce/`.** Real-hardware ESP32. Two cases (talker, listener) — service/action variants deferred until ESP32 zenoh ships them too (matrix internal-symmetry rule).
- [ ] **117.B.7 — `stm32f4/rust/xrce/`.** smoltcp UDP via the lan9118 board. Two cases (talker/listener) for symmetry with stm32f4 zenoh.

#### DDS coverage on C/C++

- [ ] **117.C.1 — `qemu-arm-freertos/{c,cpp}/dds/`.** dust-DDS is already on the Rust talker/listener for FreeRTOS; the C ABI from `nros-c`/`nros-cpp` reaches it without backend changes. 12 examples.
- [ ] **117.C.2 — `qemu-arm-nuttx/{c,cpp}/dds/`.** Same shape. 12 examples.
- [ ] **117.C.3 — `qemu-riscv64-threadx/{c,cpp}/dds/`.** Same shape. 12 examples.
- [ ] **117.C.4 — `threadx-linux/{c,cpp}/dds/`.** Same shape. 12 examples.

#### Service / action variants on Rust-on-RTOS cells

- [ ] **117.D.1 — `qemu-arm-baremetal/rust/zenoh/`** add `service-{client,server}` + `action-{client,server}`. Today only `talker`+`listener` and the rtic specials. Four examples.
- [ ] **117.D.2 — `qemu-arm-baremetal/rust/dds/`** add the same four.
- [ ] **117.D.3 — `qemu-esp32-baremetal/rust/{zenoh,dds}/`** add the same four × two RMWs = 8.
- [ ] **117.D.4 — `qemu-arm-freertos/rust/dds/`** add the same four.
- [ ] **117.D.5 — `qemu-arm-nuttx/rust/dds/`** add the same four.
- [ ] **117.D.6 — `qemu-riscv64-threadx/rust/dds/`** add the same four.
- [ ] **117.D.7 — `threadx-linux/rust/dds/`** add the same four.
- [ ] **117.D.8 — `stm32f4/rust/zenoh/`** add `listener` + service/action quartet (today only talker + rtic). Five examples.
- [ ] **117.D.9 — `esp32/rust/zenoh/`** add service/action quartet. Real-hardware constraints — bench on ESP32-WROOM. Four examples.

### Tier 3 — Out-of-scope documentation

- [ ] **117.E.1 — Bare-metal C/C++ holes.** Document in `examples/README.md` and the CLAUDE.md "Examples = Standalone Projects" section that `qemu-arm-baremetal/{c,cpp}`, `qemu-esp32-baremetal/{c,cpp}`, `esp32/{c,cpp}`, `stm32f4/{c,cpp}` are intentionally empty pending a future bare-metal C harness phase. Cite `nros-c`'s "hosted RTOS only" contract from the C-API guide.
- [ ] **117.E.2 — `px4/{c,cpp}` hole.** Document that PX4 examples are uORB-only, and the uORB API surface is Rust-only by upstream design. C/C++ uORB shim is not on any roadmap.

### Tier 4 — Coverage tests + CI

- [ ] **117.F.1 — `nros_tests::matrix` integration test.** A single nextest that drives `tools/example-matrix.py --lint` and asserts no untriaged cells. Catches future drift the same way `just check` catches Cargo workspace drift.
- [ ] **117.F.2 — Per-cell smoke run flag.** `nros_tests` already has per-platform groups; this work item adds a `matrix` group that just *builds* every example (no run) on every PR. `just test-all` retains the actual run-and-assert tier for the platforms that have a test harness.

---

## Files

```
tools/example-matrix.py                            (new, 117.A.1)
examples/README.md                                 (rewritten, 117.A.2 + 117.E.x)
examples/qemu-arm-freertos/c/xrce/{6 cases}/       (new, 117.B.1)
examples/qemu-arm-freertos/cpp/xrce/{6 cases}/     (new, 117.B.1)
examples/qemu-arm-freertos/rust/xrce/{6 cases}/    (new, 117.B.1)
examples/qemu-arm-nuttx/{c,cpp,rust}/xrce/...      (new, 117.B.2)
examples/qemu-riscv64-threadx/{c,cpp,rust}/xrce/...(new, 117.B.3)
examples/threadx-linux/{c,cpp,rust}/xrce/...       (new, 117.B.4)
examples/qemu-esp32-baremetal/rust/xrce/...        (new, 117.B.5)
examples/esp32/rust/xrce/{talker,listener}/        (new, 117.B.6)
examples/stm32f4/rust/xrce/{talker,listener}/      (new, 117.B.7)
examples/qemu-arm-freertos/{c,cpp}/dds/{6 cases}/  (new, 117.C.1)
examples/qemu-arm-nuttx/{c,cpp}/dds/...            (new, 117.C.2)
examples/qemu-riscv64-threadx/{c,cpp}/dds/...      (new, 117.C.3)
examples/threadx-linux/{c,cpp}/dds/...             (new, 117.C.4)
examples/{various}/rust/{zenoh,dds}/{service-*,action-*}/  (new, 117.D.x)
packages/testing/nros-tests/tests/example_matrix.rs (new, 117.F.1)
justfile                                           (edited, 117.A.3 + 117.F.2)
```

---

## Acceptance criteria

- [ ] `tools/example-matrix.py` exists and prints the table without ad-hoc post-processing.
- [ ] `examples/README.md` carries the autogenerated table, regenerated on every cell add/remove.
- [ ] Every cell in §B is either `Y` (six canonical cases ship) or named in `examples/README.md`'s "Deliberately empty" subsection with a one-line gating reason.
- [ ] `just check-example-matrix` exits 0 on `main`; CI fails on any new untriaged cell.
- [ ] `cargo build` succeeds on every shipped example. (Run does *not* need to succeed on every platform — the existing per-platform test groups handle that tier.)
- [ ] CLAUDE.md "Examples = Standalone Projects" section gains a one-line pointer to the matrix table so future contributors don't re-derive it from the file tree.

---

## Notes

- **Why not auto-generate the canonical six?** Generation is tempting but the per-platform `Cargo.toml`, board-config and `CMakeLists.txt` differ enough that templated codegen would either produce broken examples or hide platform-specific tweaks behind a build script. Manual port-and-adjust matches the existing project rule that examples are copy-out templates, not generated artefacts.
- **Test budget.** Tier-2 ports add ~120 example crates. Build-only smoke (Tier 4) adds proportional CI minutes; `sccache` + per-platform nextest groups already cover most of the cost. The headline number for new run-time tests is small — most ports reuse the existing per-platform integration harness.
- **Custom-transport demos.** Phase 115.F's bare-metal-C variant is *not* part of this matrix; it is its own deferred work item gated on the bare-metal C harness.
- **`px4` row.** uORB stays uORB-only by construction. The matrix table prints `(uorb)` instead of three blanks for that row.
- **`esp32` real-hardware constraints.** ESP32 examples are flashed-and-run, not QEMU. Service/action variants land only when a hardware-loop CI is reachable, otherwise the cells are partial-fill (talker + listener) by design and noted as such.
