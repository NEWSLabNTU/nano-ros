# Phase 545 — `nros build` selects an RMW through the provider scan

**Status (2026-11-09). W1, W2 and W3 DONE; W4 (book) open.** Issue 2295 is
resolved and archived; RFC-0071 D16 step 3 (Zephyr) moved out to
[issue 2372](../issues/2372-out-of-tree-rmw-native-sim-west-app.md) by user
ruling. Design accepted the same day (user
ruling: it enables third-party RMW backends on the `nros build` road). Owned
by the phase-537 book session ("AUO 139 book", user ruling 2026-11-07), which
wrote the amendment. Each work item is claimed with `just claim phase-545-wN`
before it starts.
Implements [RFC-0071](../design/0071-rmw-backend-descriptor.md)
Amendment 2026-11-07, D14 to D16. Closes
[issue 2295](../issues/archived/2295-nros-build-rmw-selection-in-tree-only.md).

Depends on:

- [phase-530](phase-530-nros-configure-single-pass.md) W2 to W8, which
  rewrite `cmd/{configure,build,entity_census}.rs` and
  `cmake/NanoRos{Promise,Resolved,Diagnostics}.cmake`. Phase-530 W2 lands
  first. W1's site migrations live in files phase-530 does not touch (the
  facade, `nros_config`, `codegen_system`, `rmw_config`, `bridge_gen`,
  `west_app`, the scaffolds). Only W1's last step, computing the scan once in
  `nros build` and `nros configure` and passing it down, edits `build.rs` and
  `configure.rs`, and it rebases onto phase-530 W2.

## Why

RFC-0071 D5 promises that a backend can be added out-of-tree with no core
edit. That holds on the cmake road (issue 1214 moved `nros dev rmw dispatch`
onto `resolve_rmw_in`) and fails on `nros build` and `nros configure`. Every
CLI site still calls `resolve_rmw`, which reads `RMW_ROWS`, the table
`cargo-nano-ros/build.rs` bakes from one checkout. An image that declares
`rmw = "acme"` for a workspace provider is refused as an unknown RMW. The
amendment gives one answer (the scan), a link rule for a backend the board
crate does not name, and an acceptance test with a gate.

## Work items

### W1 — One answer: every site resolves through the scan (D14)

- Add `InTreeScope::Rmw` (the sibling of `InTreeScope::Runners`), which narrows
  root 0 to `packages/rmw/`.
- Add a small `RmwIndex` that memoises `resolve_rmw_in` by name over one scan
  per invocation. `nros build` and `nros configure` compute it once and pass it
  to every site.
- Move these sites onto it:
  - `orchestration/facade.rs` (`derive`, its extra-backend loops, and
    `render_backend_macros_inner`, whose silent `filter_map(… .ok())` becomes
    D5's refusal naming the backend and the image);
  - `orchestration/nros_config.rs` (load-time validation);
  - `cmd/codegen_system.rs` (`rmw_token`);
  - `builder/west_app.rs`;
  - `orchestration/rmw_config.rs`;
  - `orchestration/bridge_gen.rs`.
- A caller with no workspace (`nros new`'s `--rmw` check in
  `cargo-nano-ros/src/scaffold.rs` and `workspace_scaffold.rs`, and help text
  that lists backends) scans `[<sdk root>]` alone. If no SDK root resolves, it
  fails with `nano_ros_root::not_found_help()`.
- Move the capability helpers (`backend_declares_capability`,
  `backend_states_fact`, `backends_declaring`, `backend_capability_feature`,
  `canonical_rmw`, `known_rmw`) onto `ResolvedRmwIn`. `resolve_rmw` and the
  table's lookups become `#[cfg(test)]` or are deleted. `build.rs` keeps
  checking the in-tree descriptors at compile time (RFC-0087 D4).
- Unit tests first, one per site: each takes a scan holding an `acme` provider
  and asserts the out-of-tree answer.

**Acceptance:**
- every site's unit test passes;
- an in-tree image's generated output is byte-identical before and after;
- an unresolvable name is refused with D5's message, never dropped.

### W2 — Link a backend the board crate does not name (D15)

- When the board declares `rmw-<x>`, nothing changes.
- Otherwise the generated entry, or an owned entry's
  `<entry>_nros_derived`, gets a direct path dependency on
  `[rmw.provides.cargo] crate` at `ResolvedRmwIn::package_dir`, with
  `default-features = false`. Its features are the edition, the backend's
  capability features, and the board's `platform_feature` when the crate
  declares it. If the crate's `[package].name` differs from the descriptor's
  `crate`, the configure is refused.
- The facade renders `__nros_register_backends!` with these backends only.
  `nros::main!` calls it on every arm. It is empty for an image whose board
  registers its own backend, so no backend is registered twice.
- A cmake-only backend (no `[rmw.provides.cargo]`) on a cargo-rooted image is
  refused with D7's message. C and C++ images are unchanged (D9).
- `west_app`: delete `backend_dir`. The backend's directory is
  `ResolvedRmwIn::package_dir`, and the "warn and link no Rust backend" arm
  becomes a refusal.

**Acceptance:**
- every in-tree image's expansion is unchanged;
- a unit test renders the entry for an `acme` image and asserts the path
  dependency and the register macro.

### W3 — The out-of-tree fixture, its e2e, and the gate (D16)

- Fixture workspace `packages/testing/nros-tests/fixtures/out-of-tree-rmw/`:
  - `src/acme_rmw/`: a `package.xml` announcing `rmw acme`, an
    `nros-rmw.toml`, and a cargo crate on `nros-rmw` that delegates to
    `nros-rmw-zenoh`;
  - a talker package;
  - a bringup whose image is `board = "linux"`, `rmw = "acme"`.
  Nothing under `packages/` names it.
- Step 1: `nros build` builds it with no edit outside the fixture. The test
  asserts the manifest path-deps `acme_rmw` and the register macro names it.
  It runs in `check-cli-tests`.
- Step 2: the native e2e harness runs it against a router and sees the talker
  publish through backend `acme`. It runs in the native e2e lane, beside
  1214's cmake fixture.
- Step 3: the same image with `board = "native_sim"` builds through
  `west_app`. **User ruling 2026-11-09: a FOLLOW-UP, not part of this
  phase** (it needs the Zephyr SDK on its lane). Filed as
  [issue 2372](../issues/2372-out-of-tree-rmw-native-sim-west-app.md).
- Gate `check-rmw-resolve-sites` on the fast lane. It refuses `resolve_rmw(`,
  `RMW_ROWS` and the static capability helpers in
  `packages/cli/nros-cli-core/` and `packages/cli/cargo-nano-ros/src/`, except
  `rmw_resolver.rs`'s own tests and `build.rs`. Its allowlist is empty when W1
  lands.

**Acceptance:**
- steps 1 and 2 are green (step 3 moved out, issue 2372);
- the gate is on the fast lane with an empty allowlist;
- issue 2295 is archived as resolved.

**Done (2026-11-09).**
- Fixture: `packages/testing/nros-tests/fixtures/out-of-tree-rmw/`. `acme_rmw`
  builds its vtable from `RustBackendAdapter::<nros_rmw_zenoh::ZenohRmw>` and
  registers it as `acme`, with its own identifier and a wrapped `publish` that
  prints a marker on the first sample sent through it.
- The last SDK-root-only site was `facade::entry_selection`, which
  `cmd/build.rs` reaches when it generates a cargo entry; it now takes the
  workspace (`rmw_index::for_workspace`, memoised per invocation), so
  `nros build` finds a workspace provider.
- Running the image found one more gap. `acme` links zenoh, whose hosted
  `.init_array` ctor registers `zenoh` as well, so the open saw two backends
  and no selector (`Ambiguous`). `nros::main!` now bakes the image's `rmw`
  (`BakedBootConfig::new_with_rmw`) for an image whose entry registers its
  own backend (the `(image)` arm), and `nros-board-linux` reads the baked
  `rmw`, below `$NROS_RMW`, as the RTOS boards already did. Every image
  without the arm expands as before.
- Step 1: `nros-cli-core/tests/out_of_tree_rmw_build.rs` (in
  `check-cli-tests`) drives the implicit configure and `nros build --dry-run`
  on a copy of the fixture's tracked files, and asserts the generated entry
  path-deps `acme-rmw` at the provider (`default-features = false`, the
  board's `platform_feature`), asks the board for no `rmw-*`, and registers
  `::acme_rmw::register()` on the `(image)` arm. It compiles nothing: the
  image is compiled by the build stage. Negative control: with the adapter
  back on the SDK root alone the test fails with D5's "no package provides
  rmw `acme`; searched: …".
- Step 2: row `workspace-out-of-tree-rmw` (`examples/fixtures.toml`, coordinate
  `linux/rust/zenoh`, the wire `acme` delegates to) is built by `nros build`
  in the native fixture build. `nros-tests/tests/out_of_tree_rmw_e2e.rs` runs
  it against a router with no `$NROS_RMW`, sees the publish marker, and sees
  a separate native listener hear the talker. Issue 1214 left no cmake e2e
  fixture to sit beside (its acceptance was `provider_index_gate.sh` and unit
  tests), so this is the native lane's first out-of-tree RMW image.
  `fixtures-manifest.py`'s 0831 check (a row's `rmw` must be one its image
  links) now derives what an out-of-tree backend links from the provider's
  `Cargo.toml`, so the row's `zenoh` is checked, not exempted.
- Gate: `check-rmw-resolve-sites` (`just/check/rmw.just`, fast lane by
  derivation), `scripts/check/check-rmw-resolve-sites.py`, with a self-test
  on its normal path. Its exceptions are structural (`build.rs`, the
  resolver's `#[cfg(test)]` items); there is no allowlist. A `.resolve_rmw(`
  METHOD call is not a site: `cmd/config.rs` calls the knob tree's
  `resolve_rmw`, which resolves the `rmw` knob group and reads no backend
  table.

### W4 — Book

- The custom-RMW page (`book/src/extending/custom-rmw.md`) states that an
  out-of-tree backend builds on both roads, and shows the fixture's shape as
  its example. Every shell step is probed, per the book charter (RFC-0124
  Principle 14).
- `reference/` pages that list "the backends" read from the scan's answer,
  not a typed list.

**Acceptance:** `check-book-audience` passes with no new baseline row.

## Acceptance (phase)

- An `rmw = "acme"` image from a workspace provider builds and runs through
  `nros build`, with no core edit.
- No CLI site reads `RMW_ROWS`, enforced by the gate.
- Issue 2295 is resolved.

## Issues homed here (2026-11-07)

| issue | why it belongs here |
| --- | --- |
| [#2295](../issues/archived/2295-nros-build-rmw-selection-in-tree-only.md) | Resolved by W1–W3 (2026-11-09). This phase closes it: `nros build` finds an RMW provider outside the in-tree table (RFC-0071 Amendment 2026-11-07, D14 to D16). |
| [#2310](../issues/2310-board-descriptor-platform-is-closed-enum.md) | A board descriptor's `platform` is a closed enum, so no out-of-tree platform: the platform-side twin of the out-of-tree RMW this phase enables. |
| [#2372](../issues/2372-out-of-tree-rmw-native-sim-west-app.md) | RFC-0071 D16 step 3: the `acme` image on `board = "native_sim"` through `west_app`, moved out of W3 by user ruling (2026-11-09). |
