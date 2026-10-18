# Phase 506 — the API crates carry only the API

**Status (2026-10-18). Decided; W0 (#1405) landed in PR #2344.** The
maintainer took the four design rulings this phase needed — RFC-0071 D9–D12,
amendment 2026-10-18 — and the work items below are now ordered by them. The
remaining issues were filed under RFC-0071 and baselined by
`check-rmw-agnostic` when [phase-444](archived/phase-444-rmw-fix-up.md) was
archived, or are the same idea applied to the facade.

## Why this phase exists

[RFC-0071](../design/0071-rmw-backend-descriptor.md) says the core-facing crates
(`packages/core/**`, `nros`, `nros-c`, `nros-cpp`) name no backend. A backend
is selected by the build and reached through the vtable, not by name. The gate
holds that line for new code. The baselined sites are still there, and each
one is a place where adding a backend means editing the API.

The facade has the same problem one level up. `nros::` publishes runtime
machinery beside the user API (issue 1789), and nros-c still decodes node slot
0 as "no node" (issue 1405), which nros-cpp fixed with a bias in issue 0312.

## Work items

Landing order is chosen so no step waits on a later one: W1 and W2 touch no
build road, W3 changes one header family, W4 is the largest and goes last.

### W0 — nros-c keeps "no node" distinct from slot 0

- [x] [#1405](../issues/archived/1405-c-executor-node-id-zero-reads-as-no-node.md):
      **Done 2026-10-18:** measured as a real defect (a primary node on an
      extra session registered its executor entities on session 0); one
      decode helper, an `executor_bound` flag in `rclc_node_ref_t`'s padding,
      and `executor_primary_node_session.c` in `just check c`.

### W1 — the facade carries only the user API (RFC-0071 D12)

- [x] [#1789](../issues/archived/1789-nros-facade-exports-runtime-machinery.md): the RMW
      seam leaves `nros::`; macro machinery moves to a doc-hidden
      `nros::__macro`; executor raw registration to `nros::internals`. The 64
      ledger rows are re-filed against where each name now lives.
      **Done 2026-10-18:** 76 ledger rows deleted (the seam's 66 plus 10
      now-hidden `Executor` methods), 54 re-filed; it also unmasked issue
      1942.

### W2 — backend facts come from the descriptor (RFC-0071 D11)

- [x] `[rmw.provides.cargo] crate` / `register` / `type_register_crate` in
      every `nros-rmw.toml` that has a Rust registration, read by
      `rmw_descriptor.rs` into the generated table.
- [x] [#1298](../issues/archived/1298-rmw-agnostic-core-names-backends.md): the
      selection package generates `__nros_register_backends!` /
      `__nros_register_type!`; `nros-macros` calls them and lost
      `rmw_crate_ident`; the sizing module is `type_descriptor_sizing`.
      **Done 2026-10-18:** both native bridge workspaces and the Zephyr
      params image verified at run time.
- [ ] [#1300](../issues/1300-closed-backend-lists-outside-core-api.md): the
      scaffold, colcon's standalone task and `bridge_gen.rs` are derived
      (done 2026-10-18); `board.knobs.zenoh` in `nros config` moves with W3.

### W3 — configuration by role (RFC-0071 D10)

- [ ] [#1299](../issues/1299-rmw-agnostic-backend-named-config-in-api.md):
      `entry_config.h` and `zephyr/app_config.h` read `NROS_LOCATOR`,
      `NROS_DOMAIN_ID` and the `rx` / `keepalive` transport roles; each
      backend's Kconfig maps its own spellings onto them; backend-only knobs
      become registry rows with a generated header.

### W4 — a C/C++ image's backend arrives through the generated umbrella (RFC-0071 D9)

Staged, because every C/C++ road builds `nros-c` / `nros-cpp` with an
`rmw-<name>` feature today, and the features can only go once no road uses
them. Design notes measured 2026-10-18:

* The umbrella exists (`cmake/NanoRosRuntimeCrate.cmake`, `nros_ws_runtime`)
  but only for workspaces with a Rust node, and it still takes the backend as
  an `nros-cpp` feature and anchors it through a hardcoded zenoh/xrce branch.
* The register crate is not always in the descriptor's directory (XRCE's is
  `nros-rmw-xrce-cffi`), so the descriptor states where it is.
* `nros-c` forwards the platform onto zenoh (`nros-rmw-zenoh?/platform-*`,
  several nros platforms onto `platform-bare-metal`); that mapping moves into
  the backend's descriptor.
* Two umbrella flavours: a pure-C image's umbrella sits over `nros-c`, a C++
  image's over `nros-cpp`, so a small C image does not grow by the C++ FFI
  surface.

- [x] Delete `rclcpp/node.hpp`'s `#ifdef NROS_RMW_<X>` register chain (W2,
      PR #2357).
- [x] **W4a — the cmake road.** Every C/C++ configure links a generated
      umbrella that depends on the backend crate directly (descriptor `crate`,
      `crate_dir`, platform / edition / capability features) and anchors its
      `register`; `nros_feature_set` stops handing `nros-c` / `nros-cpp` an
      `rmw-<name>` feature.
      **Done 2026-10-18:** `cmake/NanoRosImageCrate.cmake` generates the
      image crate; its lib keeps the API crate's name (`nros_c` /
      `nros_cpp`), so every Corrosion target keeps its name. The Rust-node
      workspace umbrella uses the same backend split. Verified: native C/C++
      zenoh + XRCE pub/sub, service and action cells (12/12); FreeRTOS,
      ThreadX-linux and ThreadX-riscv64 C/C++ workspace entries build;
      ThreadX-linux C/C++ `entry_e2e` cells pass; the `mixed` workspace's
      Rust-node umbrella opens a zenoh session.
- [ ] **W4b — the Zephyr west road** (`zephyr/CMakeLists.txt`'s own import).
- [ ] **W4c — the integrations that run cargo themselves**: the NuttX app
      Makefile and PX4 build through the same generated crate. **s32ds stays
      on a compatibility shim** (maintainer ruling 2026-10-18: its IDE is not
      available to verify a change), tracked as its own issue.
- [ ] **W4d** — [#1297](../issues/1297-rmw-agnostic-api-crates-name-backends.md):
      `nros-c` / `nros-cpp` lose their `rmw-<name>` features, aliases, `?/`
      forwards and optional backend deps; `check-rmw-agnostic`'s baseline
      reaches zero (the s32ds shim aside, which lives outside the API crates).

## Acceptance

`check-rmw-agnostic` passes with an empty baseline, and the `nros::` export
ledger has no row whose decision is not yet applied.

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#1297](../issues/1297-rmw-agnostic-api-crates-name-backends.md) | Per-backend Cargo features, `register()` calls and `extern` declarations in nros-c and nros-cpp are RFC-0071 D2's class. `check-rmw-agnostic` baselines them. |
| [#1298](../issues/archived/1298-rmw-agnostic-core-names-backends.md) | `nros-macros` and one other core crate name backends in code, inside RFC-0071's reach. |
| [#1299](../issues/1299-rmw-agnostic-backend-named-config-in-api.md) | Backend-named configuration (`CONFIG_NROS_ZENOH_*`, `ZENOH_LOCATOR`) in the API crates is RFC-0071 D8's class. |
| [#1300](../issues/1300-closed-backend-lists-outside-core-api.md) | Closed backend lists outside core/api (the CLI scaffold, colcon's `RMW_BACKENDS`, `board.knobs.zenoh`) are beyond the gate's reach. |
| [#1405](../issues/archived/1405-c-executor-node-id-zero-reads-as-no-node.md) | `node_raw_id != 0` decodes the primary node slot as "no node" at nine nros-c registration sites, the same conflation issue 1384 fixed one layer up. |
| [#1789](../issues/archived/1789-nros-facade-exports-runtime-machinery.md) | `nros::` still exports runtime machinery beside the user API: 64 ledger rows say a name should be hidden or moved. |
| [#1942](../issues/1942-rclrs-service-info-no-counterpart.md) | Found by W1: the seam's `ServiceInfo` had masked, by name, that rclrs's per-request service metadata has no counterpart here. |
