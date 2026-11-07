# Phase 545 — `nros build` selects an RMW through the provider scan

**Status (2026-11-07). NOT STARTED.** Design accepted the same day (user
ruling: it enables third-party RMW backends on the `nros build` road). Owned
by the phase-537 book session ("AUO 139 book", user ruling 2026-11-07), which
wrote the amendment. Each work item is claimed with `just claim phase-545-wN`
before it starts.
Implements [RFC-0071](../design/0071-rmw-backend-descriptor.md)
Amendment 2026-11-07, D14 to D16. Closes
[issue 2295](../issues/2295-nros-build-rmw-selection-in-tree-only.md).

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
  `west_app`. **Decide here first** whether this step is part of this phase or
  a follow-up, since it needs the Zephyr SDK on its lane. The 2026-11-07
  ruling left that open.
- Gate `check-rmw-resolve-sites` on the fast lane. It refuses `resolve_rmw(`,
  `RMW_ROWS` and the static capability helpers in
  `packages/cli/nros-cli-core/` and `packages/cli/cargo-nano-ros/src/`, except
  `rmw_resolver.rs`'s own tests and `build.rs`. Its allowlist is empty when W1
  lands.

**Acceptance:**
- steps 1 and 2 are green, and step 3 too unless it is moved out;
- the gate is on the fast lane with an empty allowlist;
- issue 2295 is archived as resolved.

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
