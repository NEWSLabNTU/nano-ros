# Phase 168 — Zephyr RMW collapse (Kconfig + prj.conf overlays + Cargo features)

**Goal.** Collapse the per-RMW directory axis on Zephyr Rust / C
/ C++ examples the same way native + FreeRTOS + NuttX C/C++
were collapsed in Phase 118.B.x. Zephyr is structurally
different from the other RTOSes — RMW selection lives in
**three places** at once (Kconfig + prj.conf overlay + Cargo
features) — so a clean Zephyr collapse needs its own design
pass rather than a mechanical port of the freertos / nuttx
helpers.

**Status.** Not Started.

**Priority.** P2 — same priority class as Phase 167 (NuttX
Rust collapse). The other RTOSes' collapses are landed; Zephyr
is the remaining sizeable example cluster on the legacy
`<plat>/<lang>/<rmw>/<case>/` shape.

**Depends on.** Phase 118 (example-matrix collapse mechanism +
matrix lint).

---

## Why Zephyr is different

Other RTOSes select RMW at build time via **one** axis:

- **Native, FreeRTOS, ThreadX, NuttX (C/C++)**: cmake
  `-DNROS_RMW=<rmw>` cache var drives `nano_ros_link_rmw()`
  which emits the strong-stub `nros_app_register_backends()`.
- **Native, FreeRTOS Rust**: mutually exclusive
  `rmw-{zenoh,dds,xrce}` Cargo features gate the matching
  `nros-rmw-*` optional dep.

Zephyr layers Kconfig on top:

- `prj.conf` carries `CONFIG_NROS_RMW_ZENOH=y` /
  `CONFIG_NROS_RMW_DDS=y` / `CONFIG_NROS_RMW_XRCE=y` (mutually
  exclusive — exactly one is set per `west build`).
- The Zephyr module's `Kconfig.nros` wires these into the
  module's `CMakeLists.txt` to pick which staticlib bundle gets
  linked.
- The Rust example's `build.rs` calls
  `zephyr_build::export_bool_kconfig()` so the same Kconfig
  values surface in Rust as `cfg(nros_rmw_zenoh)` / `cfg(nros_rmw_dds)` /
  `cfg(nros_rmw_xrce)`. `main.rs` already uses these in the
  legacy examples to choose which `nros_rmw_*::register()` to
  call.
- The Cargo `[dependencies]` block in each
  `<plat>/<lang>/<rmw>/<case>/Cargo.toml` then matches that
  Kconfig choice — zenoh dirs depend only on `nros-rmw-zenoh`,
  dds on `nros-rmw-dds`, xrce on `nros-rmw-xrce-cffi`.

To collapse three Zephyr dirs into one and make `west build`
select the RMW we need **all three** axes to agree, driven from
**one** input:

1. A `west build -- -DCONF_FILE="prj.conf;prj-<rmw>.conf"` overlay
   pattern that sets `CONFIG_NROS_RMW_<X>=y` for the right RMW.
2. A `Cargo.toml` with optional `nros-rmw-*` deps + matching
   `rmw-*` features.
3. A `main.rs` that uses **either** Kconfig `cfg(nros_rmw_*)`
   **or** Cargo `cfg(feature = "rmw-*")` consistently — picking
   the wrong one means the registration call won't match the
   linked staticlib.

(2) and (3) must stay in sync with (1). The natural choice is
to drive Cargo features from Kconfig via `build.rs`, but Cargo
features can't be set from `build.rs` retroactively (only
`rustc-cfg` flags can). So the practical shape is:
**Kconfig is the source of truth, Cargo features mirror it
through `west build`'s `-DCONF_FILE=` selecting a matching
`-DEXTRA_CARGO_FEATURES=` arg**.

---

## Sketch

```
examples/zephyr/rust/<case>/
├── Cargo.toml           # optional rmw-* features + deps
├── prj.conf             # base config, RMW choice unset
├── prj-zenoh.conf       # CONFIG_NROS_RMW_ZENOH=y
├── prj-dds.conf         # CONFIG_NROS_RMW_DDS=y
├── prj-xrce.conf        # CONFIG_NROS_RMW_XRCE=y
├── CMakeLists.txt       # reads CONFIG_NROS_RMW_<X>, propagates
│                        #   to EXTRA_CARGO_ARGS=--features rmw-<X>
└── src/main.rs          # #[cfg(feature = "rmw-<X>")] register
```

User-facing build:

```
west build -b native_sim -- \
    -DCONF_FILE="prj.conf;prj-dds.conf"
```

`CONF_FILE` overlay sets the Kconfig. `CMakeLists.txt` reads the
resolved `CONFIG_NROS_RMW_DDS=y` and emits
`--features rmw-dds` to Cargo via
`set(EXTRA_CARGO_ARGS --no-default-features --features
 rmw-${CONFIG_NROS_RMW_<X>_NAME})` — see
`zephyr-lang-rust`'s `rust_cargo_application()` for the existing
`EXTRA_CARGO_ARGS` knob.

Per-RMW build dirs handled by Zephyr's `west build -d build-<rmw>`
flag (already used by the per-board / per-feature variants
elsewhere).

---

## Work Items

- [ ] **168.1 — Single-case PoC on `zephyr/rust/talker/`.**
      Mirror 118.A.1's Cargo feature scaffold. Add `prj-{zenoh,
      dds,xrce}.conf` overlays. CMakeLists.txt maps Kconfig
      choice → Cargo features. Verify
      `west build -b native_sim -- -DCONF_FILE="prj.conf;prj-X.conf"`
      builds for each `X ∈ {zenoh,dds,xrce}`.
- [ ] **168.2 — Test-harness `build_zephyr_rust_example_rmw`.**
      Mirror the FreeRTOS / NuttX helpers. Per-RMW
      `build-<rmw>/` west build dir.
- [ ] **168.3 — Roll out per-case.** talker, listener,
      service-{server,client}, action-{server,client},
      `async-*` variants.
- [ ] **168.4 — C / C++ collapse.** `zephyr/{c,cpp}/<case>/`
      using the same prj.conf overlay pattern but without the
      Cargo features axis. The cmake-side `nano_ros_link_rmw`
      from Phase 144.5.c still applies — Zephyr's
      `rust_cargo_application()` plus the module's
      `Kconfig.nros` are the only additions on top.
- [ ] **168.5 — Justfile + test-harness wiring.**
      `just zephyr build-fixtures` iterates each cell × each RMW
      that the cell's Cargo.toml exposes. Smoke tests live in
      the existing `phase_118_collapse` integration test.
- [ ] **168.6 — Drop legacy `<rmw>/<case>/` siblings.** Same
      Tier 5 cleanup pattern Phase 118.E.1 establishes.

## Acceptance criteria

- [ ] Every `zephyr/<lang>/<case>/` cell builds via
      `west build` with each RMW its config declares.
- [ ] No regression on the existing
      `test_zephyr_xrce_*` / `test_zephyr_cpp_*` / `test_zephyr_dds_*`
      runtime tests.
- [ ] `phase_118_collapse` smoke includes Zephyr cells.
- [ ] CLAUDE.md "Examples = Standalone Projects" + Phase 131
      canonical shape rule both name Zephyr as fully collapsed.

## Notes

- **Why not use Cargo features as the source of truth?**
  Zephyr's RMW choice flows from Kconfig down to multiple
  consumers (the module's `CMakeLists.txt`, the staticlib bundle
  selection, the Rust example, the C / C++ examples). Switching
  the source of truth to Cargo would mean Rust drives Kconfig,
  which inverts the existing flow + breaks the C / C++ paths
  that have no Cargo at all. Mirroring through `EXTRA_CARGO_ARGS`
  preserves the existing direction.
- **What about `async-service-client` etc?** The
  `zephyr/rust/zenoh/async-service-client/` and
  `zephyr/rust/zenoh/service-client-async/` variants land in
  separate `<case>/` dirs (talker, async-service-client,
  service-client-async, …); the matrix lint treats them as
  distinct cases. 168.3 picks them up alongside the canonical
  six.
- **C++ cyclonedds.** `examples/zephyr/cpp/cyclonedds/` exists
  alongside `zephyr/cpp/{zenoh,dds,xrce}/` — a fourth RMW.
  Collapse needs `rmw-cyclonedds` feature wired in for the C++
  axis even though Rust + C don't have a cyclonedds backend yet.
- **`-DCONF_FILE="prj.conf;prj-X.conf"` overlay direction.** The
  Zephyr semantics is "later overlays win", so the base
  `prj.conf` must NOT set any `CONFIG_NROS_RMW_*=y` — only the
  overlays do. That's a one-line tweak to the existing base
  configs.
