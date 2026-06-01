# Phase 212 — UX revision: cargo-native flow + component file consolidation

**Status:** OPEN (tracking)
**Priority:** P1
**Depends on:** Phase 211 (orchestration foundation), Phase 175 (Cyclone-Rust cargo path)

## Goal

Make nano-ros's developer surface feel cargo-native for Rust users while keeping
the existing CMake path for C/C++ users. Three concrete pain points captured
from the Phase 211 design review (2026-05-31):

1. **Cyclone-Rust requires CMake.** A single-node Rust example builds with
   `cargo build` for zenoh / xrce but `cargo build --features rmw-cyclonedds`
   can't link because `nros_rmw_cyclonedds_register` lives only in a CMake-built
   sibling crate (Phase 175.A). The user accepts CMake for C/C++; Rust must be
   `cargo` end-to-end.
2. **Single-node ≠ multi-node mental model.** Single-node:
   `cargo build && cargo run`. Multi-node:
   `nros metadata → plan → check → build → deploy`. Two disjoint pipelines;
   no graduation path documented from one to the other.
3. **Component "triple-file dance".** Each component declares itself across
   `component_nros.toml` (or `nros/components/*.toml`), `metadata/*.json`,
   `package.xml`, `Cargo.toml` + `src/lib.rs` (exported FFI symbol), AND the
   launch file's `<node pkg= exec=>`. Five names must hand-align; no
   `nros check` cross-file ref validation catches a mismatch with a useful error.

## Architecture

The current shape:

```text
single-node Rust:
  cargo build → bin
  cargo run → executes
  RMW = cargo feature; zenoh/xrce OK; cyclone needs cmake

multi-node Rust:
  user writes nros.toml + component_nros.toml + metadata/*.json + Cargo.toml + src/
  nros metadata → metadata/*.json
  nros plan → record.json + nros-plan.json
  nros build → entry lib (delegates to cargo)
  nros deploy → spawn binary
```

Target shape (proposed direction, not yet committed):

```text
single-node Rust:
  cargo build → bin                          (unchanged, even for cyclonedds)
  cargo run → executes

multi-node Rust:
  cargo nano-ros plan        OR    cargo nano-ros deploy   (cargo subcommand wraps nros)
  cargo build                       (cargo discovers nros workspace via Cargo.toml metadata)
  one declaration per component, in [package.metadata.nros] of the crate's Cargo.toml
```

## Work Items

### 212.A — Cyclone-Rust under pure cargo

Phase 175.A landed CMake + Corrosion as the Cyclone-Rust example path because:
- `ddsc` is a C library; Cyclone's CMake config exports it as an imported target
  that Corrosion can hand to cargo as a link flag.
- `nros_rmw_cyclonedds_register` is a C++ symbol from a CMake-built rlib (the
  `packages/dds/nros-rmw-cyclonedds/` standalone CMake project).
- The descriptor whole-archive-link trick that keeps the static-init register TU
  alive is a `target_link_options` setting only CMake knows how to apply.

Goal: make `cargo build --features rmw-cyclonedds` work end-to-end on hosted
targets (Phase 175.B already deferred the embedded ddsrt port; 212.A targets
native_sim / x86_64).

- [ ] **Investigate `*-sys` crate for Cyclone DDS.** A `cyclonedds-sys` crate
      that runs the Cyclone CMake build in `build.rs` and emits `cargo:rustc-link-*`
      directives. Pattern matches `freertos-lwip-sys` / `threadx-netx-sys`.
- [ ] **Vendor or auto-locate `ddsc`.** Either a submodule under
      `third-party/dds/cyclonedds/` (already there for the CMake path) that
      `cyclonedds-sys`'s build.rs invokes, or a `pkg-config` lookup for a
      system Cyclone install with a clear error if missing.
- [ ] **Move `nros_rmw_cyclonedds_register` into a Rust-buildable crate.**
      Today the symbol lives in `packages/dds/nros-rmw-cyclonedds/`'s CMake
      project. Either:
      (a) make that crate buildable from cargo via `build.rs` (cc crate +
      the Cyclone idlc-generated descriptors), OR
      (b) split out a smaller Rust-side `nros-rmw-cyclonedds-rs` crate that
      registers the vtable directly via the cffi shim.
- [ ] **`examples/native/rust/talker/` cyclone variant.** Remove the
      "cyclone needs CMake" carve-out from the example's README and the
      CMakeLists.txt cyclonedds path becomes optional (CMake path stays for
      users who want to integrate into an existing CMake project).
- **Files:** `packages/dds/nros-rmw-cyclonedds/` (likely split),
  `packages/dds/cyclonedds-sys/` (new), `examples/native/rust/talker/Cargo.toml`,
  `examples/native/rust/listener/Cargo.toml`,
  `book/src/getting-started/` (drop the Cyclone-needs-CMake caveat).

### 212.B — Cargo-native multi-node workflow

The multi-node pipeline (`nros metadata/plan/check/build/deploy`) is a separate
universe from `cargo build`. Goal: make multi-node feel like a cargo workspace
to a Rust user.

- [ ] **`cargo nano-ros` subcommand.** Wrap every nros CLI verb as a cargo
      subcommand. `cargo nano-ros plan`, `cargo nano-ros deploy`, etc.
      Distribution: `cargo install cargo-nano-ros` (mirror of the existing
      `cargo-nano-ros` crate already in `nros-cli`'s workspace).
- [ ] **Cargo workspace discovery.** Today `nros plan` requires a
      `nros.toml` at the workspace root. A cargo-native flow would also accept
      a `Cargo.toml` `[workspace.metadata.nros]` table — single source for
      both rust workspace AND nros system configuration. Reuse cargo's
      workspace inheritance.
- [ ] **`cargo build` picks up nros codegen.** A `cargo nano-ros` build-script
      helper crate that, when added as a `[build-dependencies]` entry, runs
      `nros generate-rust` automatically on `cargo build`. Removes the
      `nros generate-rust --force` prerequisite step from the single-node
      flow; users get auto-codegen on first `cargo build`.
- [ ] **Document graduation path.** A book page that walks the user from
      `examples/native/rust/talker/` (single-node, pure cargo) to the same
      talker as a workspace component (multi-node, `cargo nano-ros plan/deploy`).
      Same Cargo.toml shape, two new files (workspace `nros.toml` + launch).
- **Files:** `nros-cli` (subcommand impl), `nano-ros`
  (`book/src/user-guide/single-to-multi.md`,
  `examples/templates/multi-package-workspace/`).

### 212.C — Component file consolidation

Each component today carries:
1. `package.xml` — ROS-standard, needed for ament tooling interop.
2. `Cargo.toml` (Rust) or `CMakeLists.txt` (C/C++) — needed for the
   build system.
3. `src/lib.rs` (or src/*.cpp) — the actual implementation; carries the
   `#[nros::component]` macro / `nros_component!` macro that exports the FFI
   symbol.
4. `component_nros.toml` (or `nros/components/<name>.toml` for multi-component
   packages) — declares package/component/language + `[linkage]
   crate_name/executable/exported_symbol` + `[metadata]
   source_metadata="metadata/<n>.json"` + `[overrides]
   default_namespace/parameters/remaps`.
5. `metadata/<comp>.json` — emitted by `nros metadata` from source attributes
   (the `#[nros::publisher]` / `#[nros::timer]` etc. macros).

Goal: shrink this to the minimum a user has to hand-author. Anything
derivable from source should be derived.

- [ ] **Move component declaration into `Cargo.toml`.** Replace
      `component_nros.toml` with a `[package.metadata.nros.component]` table.
      Build / planner / metadata tools read it from the manifest. One fewer
      file, one fewer name to align (Cargo enforces the crate name; the
      `[lib]`/`[[bin]]` target is the natural exported_symbol root).
- [ ] **Auto-derive `metadata/*.json`.** Today `nros metadata` runs a
      build-script-driven attribute scan and emits the JSON. Goal:
      `cargo build` runs the same scan as a `build.rs` step and emits the
      JSON into `target/nros-metadata/<crate>.json`. The planner reads that
      path directly instead of a committed `metadata/*.json`. Removes the
      committed `metadata/` dir from every component (it becomes a build
      artifact like `target/`).
- [ ] **Audit `[overrides]`.** The
      `default_namespace`/`parameters`/`remaps` table in `component_nros.toml`
      is rarely set in practice (most components inherit from launch file
      overrides). Survey existing fixtures + Autoware: is this table actually
      used? If <10% adoption, drop it; if used, move into
      `[package.metadata.nros.component.overrides]`.
- [ ] **`package.xml` still required** (ament interop), but
      auto-generate it from `Cargo.toml`'s `[package.metadata.nros]`
      `dependencies` list at codegen time. User maintains the cargo manifest;
      ament tooling gets the package.xml. (Or accept the duplication — this
      is the smallest pain of the five.)
- **Files:** `nros-cli`
  (`packages/nros-cli-core/src/orchestration/workspace.rs`,
  `metadata_build.rs`), `nano-ros` (every fixture under
  `packages/testing/nros-tests/fixtures/orchestration_*` migrates).

## Acceptance

- [ ] **Single-node Rust = `cargo build && cargo run` for ALL three RMWs**
      (zenoh, xrce, cyclonedds). No CMake step. (212.A)
- [ ] **Multi-node Rust = `cargo nano-ros plan && cargo nano-ros deploy`** —
      no separate `nros` binary install required for Rust users. (212.B)
- [ ] **One file per component for the user** — `Cargo.toml` carries the
      `[package.metadata.nros]` table; `metadata/*.json` becomes a build
      artifact; `component_nros.toml` retired. (212.C)
- [ ] **End-to-end multi-node tutorial** in the book that walks from
      single-node to multi-node without changing build tools. (212.B last
      bullet)

## Notes

- C/C++ flow stays CMake-based. 212.A/B/C target Rust UX only.
  The CMake `nano_ros_generate_interfaces()` + `find_package(NanoRos)` path
  (Phase 137 / 140 / 144) continues to serve C/C++ users.
- Component file consolidation (212.C) must not break the existing
  `nros/components/*.toml` multi-component layout — backwards-compat is
  important since several in-tree fixtures + Autoware integration depend on
  the current shape. A migration script (`nros migrate component-to-cargo`)
  may be needed.
- 212.B's `cargo nano-ros` subcommand benefits from prior `cargo-nano-ros`
  crate work in `nros-cli` (already named that way) — likely incremental
  surface expansion rather than a new crate from scratch.
- Phase 175.B (embedded Cyclone Rust w/ ddsrt port) remains research-grade
  and out of scope for 212.A; 212.A only targets hosted (native_sim / x86_64).

---

## Design Exploration Notes (2026-05-31)

The following design notes were produced by a 3-agent fan-out
investigation (cyclone-rust cargo path, cargo-native workflow,
component file consolidation) + synthesis pass. Treat as the
authoritative pre-implementation design until a work item lands,
at which point the relevant section above is updated with the
shipped shape.

## 212.A — Cyclone-Rust pure cargo

### Today's blockers

- `examples/native/rust/talker/CMakeLists.txt` only build path; plain `cargo build --features rmw-cyclonedds` fails to link.
- `nros_rmw_cyclonedds_register` lives in C++ wrapper static lib built by `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt` — no `*-sys` crate vendors it.
- `libddsc` not vendored at Cargo layer; only `find_package(CycloneDDS)` or Phase 186 self-provision `add_subdirectory(third-party/dds/cyclonedds)`.
- Host `idlc` required to emit `dds_topic_descriptor_t` per IDL; produced only by Cyclone's CMake host-build, not exposed to cargo consumers.
- Descriptor-register TU (`std_msgs__nano_ros_c`) needs `-Wl,--whole-archive` — wrapped by CMake's `nros_rmw_cyclonedds_generate_from_msg`, no cargo path.
- Baked `rmw_dds_common::ParticipantEntitiesInfo` descriptor built from `src/idl/rmw_dds_common_graph.idl` inside the CMake wrapper.
- `stdc++` + `-Wl,--allow-multiple-definition` linker discipline currently CMake-injected.
- `scripts/cyclonedds/msg_to_cyclone_idl.py` is python3 — pure-cargo users without python break.
- ddsi semi-internal headers (`q_protocol.h`, `ddsi_serdata_default.h`, etc.) needed by `sertype_min.cpp`; not exported by install tree.

### Migration plan

1. **Add `packages/dds/cyclonedds-sys/`** — vendor Cyclone via `cmake` crate against `third-party/dds/cyclonedds` (pinned 0.10.5). Force `-DENABLE_LTO=OFF`, `-DBUILD_IDLC=ON`. Separate host `cmake::Config` build for `idlc` only (cmake-crate `.build_target("idlc")`, target triple forced to host). Honor `CYCLONEDDS_PREBUILT_DIR` short-circuit. Export `links = "ddsc"`, `cargo:idlc=<path>`, `cargo:include=<...>`. Files: `packages/dds/cyclonedds-sys/{Cargo.toml,build.rs,src/lib.rs}`. **Risk: MED** — host/target split + LTO discipline already solved by cyclors; crib heavily.
2. **Move C++ wrapper into `packages/dds/nros-rmw-cyclonedds-sys/build.rs`** — `cc::Build::cpp(true)` over existing `packages/dds/nros-rmw-cyclonedds/src/*.cpp`; consume `DEP_DDSC_*`; flags `-fno-exceptions -fno-rtti -ffunction-sections`. Bake `rmw_dds_common_graph` descriptor via bundled host `idlc`. Emit `cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds` + `dylib=stdc++`. Keep CMake project alive for C/C++ examples. Files touched: `packages/dds/nros-rmw-cyclonedds-sys/{Cargo.toml,build.rs,src/lib.rs}`. **Risk: HIGH** — semi-internal headers brittle across Cyclone bumps + `linkme` registrar symbol-visibility.
3. **Per-example descriptor codegen** — extend `nros generate-rust` (or new `nros generate cyclonedds-descriptors`) to emit small generated crate (idlc C output + register TU). Example pulls via `[build-dependencies]` + `[dependencies]`. Replace `examples/native/rust/talker/CMakeLists.txt`. Files: `nros-cli` codegen + `examples/native/rust/{talker,listener}/{Cargo.toml,build.rs}`. **Risk: MED** — codegen plumbing exists in nros-cli; idlc invocation new.
4. **Flip Phase 181 fixture matrix** to include `(native, rust, cyclonedds)` via plain `cargo build`. Retire CMakeLists for native rust cyclonedds. Update CLAUDE.md Phase 175 paragraph; archive 212.A doc. Files: `packages/testing/nros-tests/fixtures/`, `examples/native/rust/talker/`, `CLAUDE.md`. **Risk: LOW** once 1–3 land.
5. **Port `msg_to_cyclone_idl.py` to Rust build-dep** OR ship pre-generated IDL beside `package.xml`. Files: `scripts/cyclonedds/` or new `packages/nros-msg-to-idl/`. **Risk: LOW** — script ~200 LoC.

### Decision points

- **`cyclonedds-sys` published to crates.io, or in-tree only?** Trade: published lets external Rust users consume nano-ros standalone; in-tree avoids release cadence + cyclors overlap. Recommend in-tree first, publish after wrapper stabilizes.
- **Vendor Cyclone source, or cyclors dep?** Trade: cyclors is ZettaScale-blessed (zenoh-plugin-dds uses it) — adopt = less code, less control; vendor = matches pinned 0.10.5 submodule + Phase 117 wire-compat assumptions. Recommend vendor (own submodule, pinned).
- **Python build-dep tolerable?** If yes, ship `msg_to_cyclone_idl.py` invocation; if no, port to Rust (step 5). Recommend port — purity goal of 212.A is "no extras."
- **Cross-compile story?** Embedded Cyclone (ThreadX 177.22, Zephyr 184.8) still CMake-driven; pure-cargo path host-only initially? Or thread NSOS/ThreadX support through build.rs day-one?
- **CMake path retire vs. coexist?** C/C++ examples still need CMake. Keep CMake project as second consumer of same source; sys crate becomes third. Cost: two build systems to test.

## 212.B — Cargo-native workflow

### Existing cargo-nano-ros crate

Lives at `/home/aeon/repos/nros-cli/packages/cargo-nano-ros/`. **Library only** (`[lib] name = "cargo_nano_ros"`, no `[[bin]]`). README states `cargo nano-ros` subcommand retired. Exposes codegen primitives (`ament_installer`, `cache`, `config_patcher`, `dependency_parser`, `package_discovery`, `package_xml`, `scaffold`, `workflow`) consumed by `nros` CLI. **Squatted name, no cargo-subcommand affordance** — clean slate to add real `cargo-nros` binary; lib should rename to `nros-codegen-lib` or fold into `nros-cli-core`.

### Proposed `cargo nros` surface

```
cargo nros plan <pkg> <launch.xml>     # → nros plan
cargo nros metadata --workspace .      # → nros metadata
cargo nros build [-- cargo-args]       # codegen + cargo build
cargo nros deploy <target>             # → nros deploy <target>
cargo nros run <target> -- <argv>      # build + flash + monitor
cargo nros generate-rust [--force]     # → nros generate-rust
cargo nros check                       # → nros check (strict, deny_unknown_fields)
cargo nros setup <board|--tool|--source> # → nros setup (passthrough)
```

Binary `cargo-nros` (~60 LoC clap shell), strips cargo-injected `nros` argv[1], dispatches to `nros_cli_core::cmd::*::run`. Global flags `--manifest-path/--verbose/--quiet/--offline/--frozen/--locked` (clap `global = true`).

### Cargo.toml [workspace.metadata.nros] schema

```toml
[workspace.metadata.nros]
default-deploy = "native"

[workspace.metadata.nros.system]
launch     = "src/demo_pkg/launch/system.launch.xml"
components = ["demo_pkg"]
rmw        = "zenoh"
domain_id  = 0

[workspace.metadata.nros.deploy.native]
kind   = "self"
target = "x86_64-unknown-linux-gnu"

[workspace.metadata.nros.deploy.qemu-freertos]
kind   = "qemu"
board  = "mps2-an385"
target = "thumbv7m-none-eabi"

[[workspace.metadata.nros.domain]]
id   = 0
rmw  = "zenoh"
nodes = ["talker", "listener"]

[[workspace.metadata.nros.bridge]]
from = { domain = 0, topic = "/chatter" }
to   = { domain = 1, topic = "/chatter" }
```

Reads via `cargo_metadata::MetadataCommand::new().exec().workspace_metadata["nros"]` → fed into existing `NrosConfig` loader (shim `NrosConfig::from_cargo_metadata`). Fallback to `nros.toml` if metadata absent. Byte-for-byte same TOML keys as today's `nros.toml`.

### Auto-codegen mechanism

New crate `packages/nros-build/` (lib). Consumer uses as build-dep:

```toml
[build-dependencies]
nros-build = "0.x"
```

```rust
// build.rs
fn main() {
    nros_build::Codegen::new()
        .package_xml("package.xml")
        .language(nros_build::Lang::Rust)
        .out_env("NROS_GEN_DIR")
        .emit_rerun()
        .run().unwrap();
}
```

Internals: resolve `nros` binary via `$NROS_BIN` → PATH → `~/.nros/bin/nros` (mirrors `scripts/build/cargo.sh::nros_cli_bin`). Shell out to `nros codegen --args-file <json>`. Write to `$OUT_DIR/nros-gen/` only (never `target/nros/...` — preserves `--target-dir` isolation rule). Emit `cargo:rerun-if-changed=` for `package.xml` + every `.msg`/`.srv`/`.action` + interface-package roots. SHA-256 input digest at `$OUT_DIR/nros-gen/.stamp` (reuse Phase 195 `cache` module). Degrades to no-op when `CARGO_FEATURE_*` selects no RMW. Missing `nros` binary → hard fail with install pointer.

### Graduation path

1. **Single-node talker** — `examples/native/rust/talker/` standalone crate, `cargo run`.
2. **Add auto-codegen** — drop `[build-dependencies] nros-build` + 6-line `build.rs`; delete manual `nros generate-rust` step. Still single-crate.
3. **Add deploy targets** — append `[package.metadata.nros.deploy.native]` to crate `Cargo.toml`; `cargo nros deploy native` runs build+spawn.
4. **Promote to workspace** — move `talker/` to `src/demo_pkg/talker/` under workspace root; root `Cargo.toml` adds `[workspace] members + [workspace.metadata.nros]`; add `src/demo_pkg/launch/system.launch.xml`; add `package.xml` for discovery.
5. **Multi-node system** — add `listener/` member, list in `[workspace.metadata.nros.system].components`, `cargo nros plan` emits `build/demo_pkg/nros/plan.json`, `cargo nros deploy native` orchestrates per-node cargo builds + spawn. Bridges/domains via `[[workspace.metadata.nros.bridge]]` / `[[...domain]]`.

## 212.C — Component file consolidation

### Field-derivation matrix

| field | source today | derivable from | proposed home |
|---|---|---|---|
| `package` | `component_nros.toml` | `Cargo.toml.package.name` | derived |
| `component` (short) | `component_nros.toml` | `#[nros::component]` ident in src | derived |
| `language` | `component_nros.toml` | toolchain (Cargo→rust, CMake→c/cpp) | derived |
| `linkage.crate_name` | `component_nros.toml` | `Cargo.toml.[lib]`/`package.name` | derived |
| `linkage.executable` | `component_nros.toml` | `Cargo.toml.[[bin]].name` or short name | derived |
| `linkage.exported_symbol` | `component_nros.toml` | convention `nros_component_<short>` | derived |
| `linkage.static_library` | `component_nros.toml` | Cargo `[lib]` crate-type | derived |
| `metadata.source_metadata` | `component_nros.toml` | convention `target/nros-metadata/<short>.json` | derived |
| `metadata.generated_by` | `component_nros.toml` | macro writes `trace.generator` | derived |
| `overrides.default_namespace` | `component_nros.toml` | none (deployment intent) | `[package.metadata.nros.component]` |
| `overrides.parameters` | `component_nros.toml` | none | `[package.metadata.nros.component]` |
| `overrides.remaps` | `component_nros.toml` | none | `[package.metadata.nros.component]` |
| `metadata/*.json` content | `metadata/*.json` | metadata-mode build artifact | `target/nros-metadata/` (gitignored) |
| `package.xml.<name>` | `package.xml` | `Cargo.toml.package.name` | regenerated |
| `package.xml.<version,desc,license>` | `package.xml` | `Cargo.toml.[package]` | regenerated |
| `package.xml.<build_depend,exec_depend>` | `package.xml` | none (ament-only ROS deps) | `[package.metadata.ament]` |
| `nodes/callbacks/parameters/pubs/subs/timers/services/actions` | `metadata/*.json` | `#[nros::component]` macro on src | macro → `target/` |

### Proposed end-state per-component layout

Before (single-component, 5 files):
```
demo_pkg/
  Cargo.toml
  package.xml
  component_nros.toml
  metadata/
    talker.json
  src/
    lib.rs
```

After (single-component, 2 files):
```
demo_pkg/
  Cargo.toml          # + [package.metadata.nros.component] + [package.metadata.ament]
  src/
    lib.rs
  target/             # gitignored
    nros-metadata/
      talker.json     # regenerated by `nros metadata --build`
```

Before (multi-component, N=2 → 7 files):
```
demo_pkg/
  Cargo.toml
  package.xml
  nros/
    components/
      talker.toml
      listener.toml
  metadata/
    talker.json
    listener.json
  src/
```

After (multi-component, 2 files):
```
demo_pkg/
  Cargo.toml          # + [package.metadata.nros.components.Talker] + [...Listener]
  src/
```

### Consolidated Cargo.toml [package.metadata.nros] schema

```toml
[package]
name = "demo_pkg"
version = "0.1.0"
description = "demo nano-ros component"
license = "MIT"

# Single-component shape
[package.metadata.nros.component]
default_namespace = "/"
parameters = { rate_hz = 10 }
remaps = [{ from = "chatter", to = "demo/chatter" }]

# Multi-component shape (table-of-tables; mutually exclusive with .component)
[package.metadata.nros.components.Talker]
default_namespace = "/"
parameters = {}

[package.metadata.nros.components.Listener]
default_namespace = "/"

# ament-only data (regenerates package.xml on `nros emit package-xml`)
[package.metadata.ament]
maintainer = "nano-ros <noreply@example.com>"
build_depend = ["rosidl_default_generators"]
exec_depend = ["rosidl_default_runtime"]
```

### Multi-component packages

`nros/components/<x>.toml` collapses to `[package.metadata.nros.components.<Name>]` table-of-tables. Short name = TOML key; id = `<crate>::<Name>`; exported symbol = `nros_component_<crate>_<Name>` (already convention in composable fixture). Cross-language carve-out: when a component is implemented in a sibling C/C++ crate (no `Cargo.toml`), keep the per-component `[component]` in `nros.toml` (Phase 172 W.1 already supports it). Rust-only multi-component packages always use the table-of-tables form.

### Irreducible minimum

- `Cargo.toml.[package].name` — cargo requires.
- `[package.metadata.nros.component].{default_namespace,parameters,remaps}` — pure deployment intent, no source signal.
- `[package.metadata.ament].{build_depend,exec_depend}` for non-cargo ROS deps — cargo has no slot.
- `#[nros::component]` macro annotations in `src/` — metadata generator needs them.

Everything else folds in or moves to `target/`. Net: **5 → 2** single-component, **3 + 2N → 2** multi-component.

## Cross-cutting concerns

- **Existing fixtures backwards compat:** `packages/testing/nros-tests/fixtures/orchestration_e2e/nros.toml` + every `component_nros.toml` need migration. Provide `nros migrate component-to-cargo` (212.C) + `nros migrate nros-toml-to-cargo` (212.B) — idempotent, re-runnable. Fallback loaders (`nros.toml`, `component_nros.toml`) survive one full release cycle.
- **Ament/colcon interop:** `nros emit package-xml` regenerates `package.xml` from `[package.metadata.ament]` so colcon workspaces still build. PX4/Zephyr integrations under `integrations/<rtos>/` consume CMake — unaffected by 212.B/C; 212.A's CMake wrapper stays alive for them.
- **CMake examples / C/C++ surface:** 212.A sys crate is third consumer of `packages/dds/nros-rmw-cyclonedds/src/*.cpp`. CMake project + Corrosion path remain canonical for `examples/<plat>/cpp/`. Test matrix must cover both build paths to prevent drift.
- **Doc rewrites:** `CLAUDE.md` "Examples = Standalone Projects" + "CMake Path Convention" + "Phase 175" paragraph + `book/src/reference/build-commands.md` + `book/src/internals/rmw-backends.md` all reference today's shape. Touch under 212.B step "convert one example" and 212.A step 4.
- **`scripts/build/cargo.sh` + `just` recipes:** thin wrappers around cargo — when `cargo nros` lands, `just` recipes become `cargo nros` dispatchers (preserve tier semantics).
- **Probe-only opaque sizes (Phase 118.B):** `nros-build` codegen must tolerate `cargo check --no-default-features` — degrade to no-op same as today's probe.
- **Parallel-feature target-dir isolation rule:** `nros-build` writes to `$OUT_DIR` only — preserves `target-safety/`/`target-zero-copy/` isolation (CLAUDE.md).
- **Workspace-metadata typo safety:** cargo ignores unknown keys in `[workspace.metadata.*]` / `[package.metadata.*]`; `cargo nros check --strict` runs `#[serde(deny_unknown_fields)]` pass to catch typos.
- **`~/.nros/bin` on PATH:** `cargo-nros` discovery needs it (`scripts/install-nros.sh` already does for `nros`); ship `cargo-nros` from same release artifact.
- **Migration tooling lives in `nros-cli` repo:** `nros migrate component-to-cargo`, `nros migrate nros-toml-to-cargo`, `nros emit package-xml`, `nros metadata --build` — single CLI, three verbs, ship together.

## Recommended execution order

1. **212.B — `cargo-nros` binary shell** (S) — ~60 LoC clap dispatcher in `nros-cli` repo, re-exports `nros_cli_core::cmd::Cmd`. Zero behavior change; install via `scripts/install-nros.sh`.
2. **212.B — `[workspace.metadata.nros]` loader** (M) — `NrosConfig::from_cargo_metadata` shim + fallback to `nros.toml`. Test with `orchestration_e2e` fixture migrated.
3. **212.B — `nros-build` crate** (M) — build-dep helper with `Codegen` builder; degrade-to-no-op + stamp-cache. Convert `examples/native/rust/talker/` as proof.
4. **212.C — migration tooling** (M) — `nros migrate component-to-cargo` + `nros emit package-xml` + `[package.metadata.nros.component]` schema in `nros-cli-core`. Single-component first.
5. **212.C — multi-component table-of-tables** (S) — extend loader to read `[package.metadata.nros.components.<Name>]`; migrate composable fixture.
6. **212.A.1 — `cyclonedds-sys`** (L) — vendor Cyclone via `cmake` crate + host `idlc` split. Standalone PR, no consumers yet.
7. **212.A.2 — `nros-rmw-cyclonedds-sys` wrapper build** (L) — move C++ wrapper into build.rs; bake `rmw_dds_common_graph` descriptor. Highest-risk step.
8. **212.A.5 — port `msg_to_cyclone_idl.py` to Rust** (S) — kill python build-dep.
9. **212.A.3 — per-example descriptor codegen** (M) — extend `nros generate-rust` or new `nros generate cyclonedds-descriptors`.
10. **212.A.4 — flip native rust cyclonedds matrix to pure cargo** (S) — retire `examples/native/rust/talker/CMakeLists.txt`; archive Phase 212.A doc.
11. **Doc + CLAUDE.md sweep** (S) — rewrite "Examples = Standalone Projects" + Phase 175 paragraph + build-commands.md + rmw-backends.md.
12. **Retire fallback loaders** (S) — one release after step 4; delete `component_nros.toml` / `nros.toml` parser paths.
