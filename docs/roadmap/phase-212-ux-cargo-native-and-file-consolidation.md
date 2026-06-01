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

---

## Architecture Decision: build-system-native (2026-05-31)

**Decision:** nros = idf.py-shaped provisioner + codegen + metadata reader.
**Cargo and CMake stay user-facing build verbs. nros NEVER is.**

Studied four reference models (colcon/ament, Bazel/Buck2, cargo-make/just/mage,
west/idf.py) via 4-agent fan-out workflow `wwv0lmnq2`. Closest fit in spirit:
**idf.py**. Rationale:
- ESP-IDF cleanly separates cmake (build) from idf.py (embedded affordances:
  flash, monitor, set-target). cmake stays cmake; idf.py adds without owning.
- Colcon's orchestrator-driven model swallows rustc/gcc errors via aggregated
  `Failed <<<` reporters and forces every consumer to learn the orchestrator.
  Embedded Rust contributors flee that pattern.
- Bazel-shape only pays off at 1M LoC + remote cache + dedicated build team.
  nano-ros has none.

### Non-Goals

The following verbs are **explicitly rejected**:
- `nros build`
- `nros test`
- `nros flash`
- `nros monitor`
- `nros sign`

Any future temptation to add one cites this section and the
"orchestrator-owns-stdout" anti-pattern. Build verbs belong to cargo / cmake /
west / colcon. nros provides the data they read + the codegen they call.

### Where nros sits in the stack

```
USER SHELL
  cargo build │ cmake --build │ colcon build │ west build      ← user-facing
       │            │              │              │
       ▼            ▼              ▼              ▼
  build.rs       cmake          ament          zephyr
  nros-build     nano_ros_*     hooks          module
       │            │              │              │
       └────────────┴──────┬───────┴──────────────┘
                           ▼
                    ┌────────────┐
                    │   nros CLI │   ← provisioner + codegen + metadata + deploy
                    │ (prebuilt) │     NEVER a build verb
                    └────────────┘
```

### Glue surface budget (HARD caps)

| Glue | LoC budget |
|---|---|
| `packages/nros-build/` build-dep crate | ≤500 |
| `cmake/nano_ros_generate_interfaces.cmake` | ≤300 |
| `cmake/nano_ros_workspace_metadata.cmake` (new 212.D) | ≤150 |
| `cmake/platform/nano-ros-<plat>.cmake` (×6) | ≤200 each |
| `cmake/board/nano-ros-board-<board>.cmake` (×N) | ≤100 each |
| `cargo-nros` binary | ≤100 |
| `scripts/install-nros.sh` | ≤150 |
| Per-RTOS integration shells `integrations/<rtos>/` | ≤200 each |

**If `nros-build` crosses 500, redesign. If a cmake function starts parsing
Cargo.toml, redesign.**

### 212.D — cmake-side mirror of 212.B (NEW)

C/C++ users must not be second-class. Sibling of 212.B's
`[workspace.metadata.nros]` + `nros-build` Rust path:

- [ ] **`cmake/nano_ros_workspace_metadata.cmake`** — new function
      `nano_ros_workspace_metadata(LAUNCH … COMPONENTS … RMW … DOMAIN_ID …)`
      callable from top-level `CMakeLists.txt`. Shells `nros plan` with the
      same args nros-build does for Rust.
- [ ] **Acceptance:** multi-node C/C++ workspace builds with
      `cmake -S . -B build && cmake --build build` + `nros deploy`. No
      `cmake nros` subcommand (cmake has no plugin idiom — don't fake one).
- **Files:** `cmake/nano_ros_workspace_metadata.cmake`,
  `book/src/user-guide/multi-node-cpp.md`,
  `examples/templates/multi-package-workspace-cpp/`.

### Diffs vs the original Design Exploration Notes

**Stand (keep as-is):**
- 212.B `cargo nros` subcommand shape (~60 LoC clap shell)
- 212.B `[workspace.metadata.nros]` schema
- 212.B `nros-build` build-dep crate (now w/ explicit ≤500 LoC cap)
- 212.C field-derivation matrix + 5→2 / 7→2 file collapse
- 212.C `nros emit package-xml` regeneration
- Execution order steps 1–5 (low-risk cargo-native + consolidation FIRST)

**Reframe:**
- 212.B "auto-codegen": `nros-build` writes to `$OUT_DIR/nros-gen/` ONLY
  (preserves `--target-dir` isolation rule from CLAUDE.md). Promoted to
  acceptance bullet.
- 212.B `cargo nros` surface: add `cargo nros --explain <verb>` requirement
  per verb. Reject any verb that hides a cargo/cmake invocation without
  decomposition.
- 212.B `[workspace.metadata.nros]` schema: strict subset of
  `nros-sdk-index.toml` vocabulary. No second TOML dialect. Field names must
  already exist in existing configs.

**Strike (explicitly retire):**
- Original "12. retire fallback loaders one release after step 4 — delete
  `component_nros.toml` / `nros.toml` parser paths" — KEEP fallback loaders
  permanently. The `[component]` block in `nros.toml` (Phase 172 W.1) is
  the cross-language carve-out for non-cargo C/C++ siblings; can't be
  retired.
- Original "`just` recipes become `cargo nros` dispatchers" line in
  cross-cutting concerns — STRIKE. `just` recipes stay shell-direct calls
  to cargo / cmake. Contributor CI is not user-facing API; conflating the
  two reproduces the colcon anti-pattern.

**Sharpen:**
- 212.A step 2 (HIGH risk wrapper sys move): add fallback acceptance — if
  the `nros-rmw-cyclonedds-sys` build.rs proves too brittle across Cyclone
  bumps, keep CMake path as canonical for cyclonedds. Don't force a
  Rust-only path if upstream semi-internal headers churn.
- 212.A.5 python port: promote to PREREQUISITE — must land before
  step 2. Python build-dep is a regression for the "pure cargo" promise.

### Updated execution order

S → M → L by effort:

1. **212.B cargo-nros binary shell** (S) — ≤100 LoC clap dispatcher
2. **212.B `[workspace.metadata.nros]` loader** (M) — `NrosConfig::from_cargo_metadata` shim + nros.toml fallback
3. **212.B `nros-build` crate** (M) — build-dep helper, ≤500 LoC, `$OUT_DIR/nros-gen/`, SHA-256 stamp
4. **212.D `nano_ros_workspace_metadata()` cmake function** (M) — cmake sibling of 212.B
5. **212.C migration tooling** (M) — `nros migrate component-to-cargo` + `nros emit package-xml`
6. **212.C multi-component table-of-tables** (S) — `[package.metadata.nros.components.<Name>]`
7. **212.A.5 port `msg_to_cyclone_idl.py` to Rust** (S) — prereq for 212.A
8. **212.A.1 `cyclonedds-sys`** (L) — vendor Cyclone via cmake crate + host idlc split
9. **212.A.2 `nros-rmw-cyclonedds-sys`** (L) — C++ wrapper into build.rs. HIGH risk; fallback to CMake on brittleness.
10. **212.A.3 per-example descriptor codegen** (M)
11. **212.A.4 flip native rust cyclonedds matrix** (S)
12. **Doc + CLAUDE.md sweep** (S)


---

## Appendix A: Full Recommendation Document (2026-05-31)

The recommendation document produced by the architecture-study workflow
follows verbatim. Treat as the source of truth until any of its specifics
are explicitly overridden above.

## Decision: nros = idf.py-shaped provisioner + codegen + metadata reader; cargo and cmake stay the user-facing build verbs (build-system-native, NOT orchestrator-driven).

## The user workflow (post-revision)

### Single-node Rust user

```
1. cargo new my_talker                          # cargo
2. # edit Cargo.toml: add nros-node, nros-build (build-dep), pick RMW feature
3. cargo build                                  # cargo → build.rs → nros-build → shells nros codegen
4. cargo run                                    # cargo
```

cargo own top-level. `nros` invoked transitively by `build.rs` via `nros-build` helper. No `nros` verb in user shell. Works zenoh / xrce / cyclonedds same way (post-212.A).

### Single-node C/C++ user

```
1. mkdir my_talker && cd my_talker
2. # write CMakeLists.txt: set NANO_ROS_PLATFORM/NANO_ROS_RMW; add_subdirectory(<nano-ros>)
3. cmake -S . -B build                          # cmake → nano_ros_generate_interfaces() → shells nros codegen
4. cmake --build build                          # cmake
5. ./build/my_talker                            # raw exec
```

cmake own top-level. `nano_ros_*()` cmake funcs shell to same `nros` CLI binary. clangd reads `compile_commands.json` direct.

### Multi-node Rust workspace

Diff vs single-node: add workspace root `Cargo.toml` with `[workspace.metadata.nros]` + launch xml. No new build tool.

```
1. cargo new --lib demo_pkg                     # cargo
2. # workspace Cargo.toml: [workspace.metadata.nros.system] launch=… components=[…]
3. # add src/demo_pkg/launch/system.launch.xml
4. cargo nros plan                              # cargo subcommand → wraps `nros plan`
5. cargo build                                  # cargo (per-member, auto-codegen via nros-build)
6. cargo nros deploy native                     # cargo subcommand → wraps `nros deploy`
```

Same `cargo build` works. `cargo nros` = thin cargo subcommand binary. `cargo nros --explain deploy` prints underlying `nros deploy …`. No `cargo nros build` verb — decompose to `cargo nros plan && cargo build`.

### Multi-node C/C++ workspace

Diff vs single-node: top-level `CMakeLists.txt` aggregates components + calls `nano_ros_workspace_metadata()` (new, sibling to Rust path).

```
1. mkdir ws && cd ws; create subdirs talker/ listener/ launch/
2. # top CMakeLists: nano_ros_workspace_metadata(LAUNCH launch/system.launch.xml COMPONENTS talker listener RMW cyclonedds)
3. cmake -S . -B build                          # cmake → nano_ros_workspace_metadata() shells `nros plan`
4. cmake --build build                          # cmake
5. nros deploy native                           # direct `nros` (no `cmake nros` subcommand — cmake has no plugin idiom)
```

C/C++ users invoke `nros deploy` directly. Asymmetry is honest: cargo has subcommand convention, cmake does not. Don't fake one.

### Mixed Rust + C/C++ workspace (Autoware case)

This stress-test. Autoware = colcon workspace consuming nano-ros as CMake library. Two layers:

```
1. # outer: ament/colcon owns workspace graph
2. colcon build --packages-select my_nros_pkg   # colcon → cmake (per package)
3. # inside my_nros_pkg: CMakeLists uses nano_ros_generate_interfaces(...)
4. # inside my_rust_nros_pkg: ament_cargo / colcon-cargo plugin → cargo build → nros-build → nros codegen
5. # multi-node orchestration:
6. nros plan --workspace src/                   # direct nros (colcon/ament own outer DAG; nros owns inner system)
7. nros deploy native                           # direct nros
```

Key: nros does NOT try to be colcon. colcon owns the cross-package DAG. nros owns the nano-ros system DAG (domains, bridges, launch). Two graphs, clean seam at `nros plan`. Same `nros` binary serves Rust build.rs, cmake `nano_ros_*()` funcs, and Autoware integrators.

## Where nros lives in the stack

```
┌─────────────────────────────────────────────────────────────┐
│ USER SHELL                                                  │
│   cargo build │ cmake --build │ colcon build │ west build  │  ← user-facing build verbs
└────────┬────────────┬──────────────┬──────────────┬─────────┘
         │            │              │              │
         ▼            ▼              ▼              ▼
  ┌───────────┐ ┌───────────┐ ┌───────────┐  ┌────────────┐
  │ build.rs  │ │  cmake    │ │  ament    │  │  zephyr    │
  │ nros-build│ │ nano_ros_*│ │  hooks    │  │  module    │
  │  crate    │ │  funcs    │ │           │  │            │
  └─────┬─────┘ └─────┬─────┘ └─────┬─────┘  └──────┬─────┘
        │             │             │               │
        └─────────────┴──────┬──────┴───────────────┘
                             ▼
                    ┌────────────────┐
                    │   nros CLI     │  ← provisioner + codegen + metadata + deploy
                    │ (prebuilt bin) │     NEVER a build verb
                    └────────┬───────┘
                             │
              ┌──────────────┼──────────────────────┐
              ▼              ▼                      ▼
      ┌──────────────┐ ┌──────────────┐    ┌──────────────────┐
      │nros-sdk-index│ │ codegen      │    │ flash/monitor    │
      │   .toml      │ │ (rust/c/cpp) │    │   delegate:      │
      │ (SSoT pins)  │ │              │    │   west/idf/openocd│
      └──────────────┘ └──────────────┘    └──────────────────┘
```

nros sit BELOW build tools. Build tools call nros. nros never call build tools (except `nros deploy` spawn processes). User never type `nros build`.

## Glue surface that nano-ros maintains

| Glue | Purpose | LoC budget | Maintenance trigger |
|---|---|---|---|
| `packages/nros-build/` (build-dep crate) | Rust build.rs helper: read `[package.metadata.nros]`, shell `nros codegen`, emit `cargo:rerun-if-changed` + stamp cache | ≤500 | nros codegen CLI flag change; new interface filetype |
| `cmake/nano_ros_generate_interfaces.cmake` | C/C++ codegen entry; shells `nros codegen`; descriptors / whole-archive link | ≤300 | RMW backend addition; cmake idiom change |
| `cmake/nano_ros_workspace_metadata.cmake` (new, 212) | Cmake sibling of `[workspace.metadata.nros]`; shells `nros plan` | ≤150 | nros plan schema change |
| `cmake/platform/nano-ros-<plat>.cmake` (×6) | Per-platform link/toolchain glue | ≤200 each | New RTOS; per-RTOS upstream break |
| `cmake/board/nano-ros-board-<board>.cmake` (×N) | Per-board overlays (linker scripts, defconfig hints) | ≤100 each | New board |
| `cargo-nros` binary (in nros-cli repo) | Cargo subcommand shim; strips argv[1], dispatches to `nros_cli_core::cmd::*` | ≤100 | New `nros` verb worth surfacing to Rust users |
| `scripts/install-nros.sh` | Pinned-version installer for `nros` + `cargo-nros` to `~/.nros/bin` | ≤150 | nros-cli release bump |
| `scripts/build/cargo.sh::nros_cli_bin` | `$NROS_BIN` → PATH → `~/.nros/bin` resolution order | ≤50 | Install layout change |
| `nros-sdk-index.toml` | SSoT for every SDK pin (data, not code) | data-only | New SDK or pin bump |
| Per-RTOS integration shells `integrations/<rtos>/` | Re-export root CMake under west/idf/PlatformIO/colcon native pkg managers | ≤200 each | New RTOS pkg manager |

Hard rules: each glue piece <1k LoC. If `nros-build` cross 500, redesign. If a cmake function start parsing Cargo.toml, redesign.

## What changes from the Phase 212 design notes

**Stand (212.B / 212.C as-is):**
- 212.B `cargo nros` subcommand + `[workspace.metadata.nros]` loader + `nros-build` build-dep crate — exactly right shape. Section 7 schema + auto-codegen mechanism keep.
- 212.C field-derivation matrix + 5→2 / 7→2 file collapse — keep verbatim.
- Recommended execution order step 1–5 — keep.

**Reframe:**
- 212.B "auto-codegen mechanism": add explicit **size cap ≤500 LoC** to `nros-build`. Document as hard rule, not aspiration. Rationale: build-deps compile for every consumer; bloat = nano-ros tax on every Rust user.
- 212.B `cargo nros` surface (lines 252–261): add `cargo nros --explain <verb>` requirement, mandatory for every verb. Reject any verb that hide a cargo / cmake invocation without `--explain` decomposition.
- 212.B `[workspace.metadata.nros]` schema: add explicit rule "strict subset of `nros-sdk-index.toml` vocabulary." No second TOML dialect. Field names must already exist in `config.toml`/Kconfig/`nros.toml`.
- 212.C `metadata/*.json`: clarify the build-artifact location MUST be `$OUT_DIR/nros-gen/` not `target/nros-metadata/` (preserves parallel-feature `--target-dir` isolation rule from CLAUDE.md). Section already say this — promote to acceptance bullet.

**Add (cmake sibling, missing from current draft):**
- New work item 212.D — **cmake-side mirror of 212.B**. `nano_ros_workspace_metadata(LAUNCH … COMPONENTS … RMW …)` function reads same fields from top-level `CMakeLists.txt`, shells `nros plan` same way `nros-build` does for Rust. Symmetry mandatory; C/C++ users not second-class.
- Acceptance bullet: "Multi-node C/C++ workspace builds with `cmake -S . -B build && cmake --build build` + `nros deploy`, no separate verb."

**Retire (or never adopt):**
- Any future temptation to add `nros build`, `nros test`, `nros flash`, `nros monitor`, `nros sign`. Add explicit **Non-Goals** section to phase doc citing anti-patterns A1 (orchestrator owns stdout) + A4 (three-hats binary).
- Note 197 stopping point: `just` → `nros` migration for **provisioning only**. Do NOT extend `just` → `nros` for CI orchestration. `just ci` / `just test-all` / Phase 176 jobserver stays. Phase 212 should explicitly say so.
- 212.B line 449 ("`just` recipes become `cargo nros` dispatchers") — **strike**. `just` recipes stay shell-direct calls to cargo/cmake. Contributor CI is not user-facing API; conflating the two reproduces the colcon anti-pattern.

**Sharpen:**
- 212.A step 2 (HIGH risk wrapper move) — accept the risk but add fallback acceptance: if `nros-rmw-cyclonedds-sys` build.rs proves too brittle across Cyclone bumps, keep CMake path as canonical for cyclonedds and document the carve-out. Don't force a Rust-only path if the upstream semi-internal headers churn.
- 212.A.5 python port — promote to prerequisite (must land before step 2). Python build-dep is a regression for the "pure cargo" promise.

## Rejected alternatives

**Orchestrator-driven (`nros build` owns the graph, top-right of matrix).** Worse for nano-ros: user base split three ways (Rust embedded, C/C++ embedded, Autoware/colcon integrators), all three already trust a build tool — cargo, cmake, colcon — and the team is too small to maintain a parallel orchestrator that simultaneously learn cargo + cmake + west + idf + colcon. Best counter-argument: "but then cross-language codegen DAG is hard." Answer: that DAG is already manageable — `nros codegen` is called by build.rs / cmake / ament hooks, three thin entry points, no global graph. Bazel-shape only pay off at 1M LoC + remote cache + dedicated build team. nano-ros has none of those. The orchestrator-owns-stdout failure mode (colcon `Failed <<<` swallowing rustc errors) is exactly what embedded Rust contributors flee from; adopting it lose the audience we built nros-build to attract.

**Monolithic single-tool (Bazel / Buck2).** Worse: would force every consumer (Autoware, PX4 integrators, ESP-IDF folks) to add Bazel to their stack. They won't. `rules_foreign_cc` + `crate_universe` glue debt is real (every cargo build.rs heavy dep need a hand-written `crate.annotation` override). nano-ros has dozens of `*-sys` crates with vendored CMake builds — Bazel migration is multi-year. Best counter-argument: "remote cache wins big." Answer: we don't have shared cache infrastructure and won't build it; sccache via `RUSTC_WRAPPER` (CLAUDE.md auto-detect) cover 80% of the win at 0% of the cost.

## First 3 commits

1. **`docs(212): rewrite phase doc with build-system-native decision + non-goals`** — apply the "What changes" section above: pin build-system-native shape, add Non-Goals (no `nros build|test|flash`), strike `just`-becomes-`cargo nros`-dispatchers line, add 212.D cmake sibling work item, add `nros-build` ≤500 LoC cap + `--explain` rule + schema-subset rule + `$OUT_DIR` location.
2. **`docs(212.D): add cmake sibling work item for nano_ros_workspace_metadata()`** — spec the cmake function signature, behavior (shell `nros plan`, read from top CMakeLists), acceptance criteria (multi-node C/C++ workspace builds with cmake + `nros deploy`, no `cmake nros` subcommand). Mirror 212.B section structure.
3. **`feat(nros-build): scaffold build-dep crate with ≤500 LoC budget + stamp cache`** — first executable step from execution order (step 3): `packages/nros-build/` crate, `Codegen` builder, `$OUT_DIR/nros-gen/` writer, SHA-256 stamp cache, `cargo:rerun-if-changed` emission, no-op degrade on `cargo check --no-default-features`. Convert `examples/native/rust/talker/` zenoh variant as proof. Cyclonedds variant lands in later 212.A commits.

---

## Appendix B: Design Analysis (2026-05-31)

The 2x2 matrix + UX/flexibility scoring tables + pattern extraction that
produced the recommendation above. Cited when a design choice needs to be
justified against an alternative.

## 1. The two-axis space

```
                    monolithic single tool
                              |
   colcon ----------- west ---+--- Bazel
                              |
                              |
  native ─────────────────────┼───────────────────── orchestrator owns graph
                              |
   just / cargo-make ---------+--- idf.py
   nano-ros TODAY             |    nros build-system-native candidate
                              |    nros orchestrator-driven candidate
                              |
                    per-language plugins
```

**Top-left (monolithic + native tools).** colcon sits here — one CLI, but delegates compile to cmake/cargo/setup.py. Fits projects that need uniform `<verb> --packages-select X` across many languages where each language already has a real build system. Pain: orchestrator owns stdout but speaks no language semantics, so errors get mangled.

**Top-right (monolithic + owns graph).** Bazel, Buck2. Big monorepo, paid build team, shared remote cache. Pays off at 1M+ LOC with 100+ devs. Below that, glue debt (`crate_universe`, `rules_foreign_cc`) exceeds value.

**Bottom-left (per-language plugins + native tools).** just, cargo-make, mage, today's nano-ros, west-for-build-only. Tiny core, recipes are aliases, language tools stay native. Fits small teams shipping polyglot stacks. Pain: orchestrator is timestamp-blind, codegen reruns, cross-package invalidation is workaround city.

**Bottom-right (per-language plugins + owns provisioning DAG).** idf.py, west-the-manifest-tool, the nros build-system-native candidate (Phase 212). Declarative manifest + provisioner + thin verbs over upstream build systems. Native tools still callable. Sweet spot for embedded multi-RTOS shops.

The two nros candidates:
- **build-system-native** = bottom-right, near idf.py: cargo/cmake drive their own builds; `nros` provides metadata, codegen, provisioning; `cargo nros` is a subcommand not a replacement.
- **orchestrator-driven** = top-right corner crossing into Bazel territory: `nros build` becomes THE entry point, owns the graph across cargo + cmake + west + idf.

## 2. UX scoring

| Project | Onboard (cmds) | Error attr | IDE | Cross-lang codegen | Maint burden | Embedded toolchain fit |
|---|---|---|---|---|---|---|
| colcon | 6 | 2 (Failed <<< pkg eats stderr) | 2 (N build trees, no top-level proj) | 4 (rosidl ext pts work) | 3 (PyPI plugin zoo) | 2 (no SDK provisioning) |
| Bazel | 8+ (Bazelisk, JDK, cache warm) | 4 (action graph clear) | 3 (rust-project.json regen) | 5 (genrule first-class) | 1 (Starlark + crate_universe drift) | 2 (rules_foreign_cc brittle for vendored CMake) |
| just | 2 (clone + just) | 5 (recipes ARE shell, errors raw) | 5 (cargo/cmake direct → rust-analyzer/clangd work) | 2 (re-runs unconditionally) | 5 (recipes are 1-liners) | 3 (no manifest concept) |
| west | 3 (init, update, build) | 3 (CMake errors via py wrapper) | 4 (cmake -B build native) | 3 (Zephyr-only) | 2 (10k LOC py, breaks per release) | 5 (manifest + runners) |
| idf.py | 3 (install.sh, export.sh, build) | 4 (`-v` prints cmake cmd) | 5 (raw cmake works) | 4 (component idf_component.yml) | 4 (3k LOC, rarely breaks) | 5 (idf_tools.py separated) |
| nano-ros today | 2 (direnv allow, just setup base) | 5 (cargo/cmake direct) | 5 (cargo workspace + cmake standalone) | 2 (generate-bindings unconditional) | 4 (8k LOC justfiles, getting heavy) | 5 (`nros setup` + index) |
| build-system-native candidate | 2 (same) | 5 (cargo/cmake stay top-level) | 5 (preserved) | 4 (`[workspace.metadata.nros]` declares deps → tracked) | 5 (cargo subcommand + cmake fn, tiny core) | 5 (keep `nros setup`) |
| orchestrator-driven candidate | 3 (`nros build`) | 3 (nros owns stdout, language errors reflow) | 3 (nros must emit compile_commands + rust-project.json) | 5 (nros sees the DAG) | 2 (becomes Bazel-shaped) | 5 (keep `nros setup`) |

## 3. Flexibility scoring

| Project | Step out to native? | Cross-project reuse | Customization w/o fork | Polyglot future-proof |
|---|---|---|---|---|
| colcon | partial (leaf builds OK, lose ament_index) | yes (PyPI plugins) | 4 (extension points) | 3 (rosidl_generator_<lang>) |
| Bazel | no (Cargo.toml drifts from BUILD) | yes (BCR, rules_*) | 5 (Starlark) | 5 (rules for anything) |
| just | yes (recipes are shell) | no (justfiles are project-local) | 5 (edit the file) | 3 (no graph) |
| west | yes (`west.yml` `import:`) | partial (Zephyr ecosystem) | 4 (entry-points in manifest projects) | 2 (Zephyr-centric) |
| idf.py | yes (`-v` shows cmake, raw cmake works) | partial (idf-component-registry) | 3 (component manager) | 3 (CMake-centric) |
| nano-ros today | yes (cargo, cmake, west all callable) | no (everything in-tree) | 4 (just files editable) | 4 (Rust+C+C+++Py already coexist) |
| build-system-native candidate | yes (preserved by design) | **yes** (cargo subcommand publishable; cmake fn reusable via FetchContent) | 5 (`[workspace.metadata.nros]` per-crate) | 5 (cargo + cmake are the polyglot lingua franca) |
| orchestrator-driven candidate | partial (need `nros run cargo …`) | partial (nros plugins, new ecosystem) | 3 (must extend nros core) | 4 (nros has to learn each new lang) |

## 4. Pattern extraction

**P1 — Manifest as single source of truth.** west.yml, idf's tools.json, nros-sdk-index.toml. Adopt: keep `nros-sdk-index.toml` authoritative; add `import:`/`extends:` semantics so downstream pins can override `[source.*]` without forking.

**P2 — Provisioning orthogonal to building.** idf_tools.py vs idf.py; `nros setup --tool` / `--source` vs `cargo build`. Adopt: never fold SDK fetch into a build verb. `nros setup` stays the only thing that touches `~/.nros` / `third-party/`.

**P3 — Wrap, don't replace.** idf.py `-v` prints the cmake command; west delegates to cmake+ninja+runners; `just` recipes are shell. Adopt: `cargo nros` subcommand and `nano_ros_*()` cmake functions wrap, never hide. `nros --explain <verb>` should print the underlying cargo/cmake invocation.

**P4 — Extension points keyed by filesystem drop-in.** rosidl_generator_<lang> via `register_<lang>.cmake`; west-commands via `west.yml` entries; ament_index as flat resource dir. Adopt: codegen backends (`generator-c`, `generator-cpp`, `generator-rust`) registered by presence of a known file under `packages/codegen/<lang>/`, no core edits to add one.

**P5 — Tiered, idempotent, re-runnable provisioning.** nros already has `minimal/default/extended`. Adopt: keep the tier discipline; every new SDK must declare its tier eligibility (size ≤500MB, install ≤5min, in `just test-all`, idempotent).

## 5. Anti-pattern extraction

**A1 — Orchestrator owns stdout, speaks no language.** colcon `Failed <<< pkg` swallowing rustc diagnostics; west reflowing cmake errors. Reject: never make `nros build` the only path; cargo and cmake errors must be visible to contributors raw.

**A2 — Per-package recipe explosion.** Buildroot's `.mk` per dep, Bazel `crate.annotation` overrides per build.rs-heavy crate. Reject: `nros-sdk-index.toml` stays declarative (url/ref/dest/submodule); never grow per-source configure/build hooks. Heavy lifting stays in upstream's build system.

**A3 — Hidden configure step.** NuttX `configure.sh` writes `.config` then disappears. Reject: `nros setup` and `nros deploy` must be idempotent and re-runnable; current state must be inspectable (`nros doctor`, `nros --list`).

**A4 — Three-hats binary.** west doing manifest + build + flash + sign + spdx + sysbuild. Reject: `nros` stays a provisioner + codegen + metadata reader. Building stays cargo/cmake. Flashing stays west/idf.py/openocd. Don't grow `nros flash`, `nros monitor`, `nros sign`.

**A5 — Replacing native graph with Starlark/proprietary DSL.** Bazel BUILD files duplicating Cargo.toml. Reject: `[workspace.metadata.nros]` lives INSIDE Cargo.toml; cmake glue lives INSIDE existing CMakeLists.txt via `nano_ros_*()` functions. No parallel manifest format for things cargo/cmake already describe.

## 6. Recommendation for nano-ros

**Position: bottom-right of the matrix, idf.py-shaped.** Build-system-native, per-language plugins, manifest-driven provisioning, native build tools stay the entry point.

Concrete decisions:

- **Rust users: cargo drives the top-level build.** `cargo build`, `cargo test`, `cargo check` work in any nros workspace without `nros` being installed for the build step. `cargo nros generate` / `cargo nros deploy` are subcommands that fire BEFORE `cargo build` for codegen + config baking. rust-analyzer sees a real cargo workspace; no `rust-project.json` regeneration.
- **C/C++ users: cmake drives the top-level build.** `cmake -S . -B build && cmake --build build` works standalone. `nano_ros_generate_interfaces(...)`, `nros_platform_link_app(...)` are cmake functions that internally shell to the `nros` CLI for codegen (same binary, two front-ends). clangd sees a real `compile_commands.json`; no extraction layer.
- **ONE entry point per language community, not one globally.** cargo for Rust, cmake for C/C++, west for Zephyr apps, idf.py for ESP-IDF apps. `nros` itself is the provisioner + codegen + metadata reader, not the build verb. Today's `just` becomes a contributor-side menu (`just ci`, `just test-all`), not a user-facing API.
- **Verb discovery: `nros --list` for provisioning/codegen; `cargo --list` shows `nros` as a subcommand; cmake users discover via the per-RTOS integration READMEs (`integrations/<rtos>/`) which already exist.** No global "list every verb across every tool" UX — that's the orchestrator-owns-everything trap.

Why this serves nano-ros specifically: the user base is split across Rust embedded folks (who expect `cargo build` to Just Work), C/C++ embedded folks (who expect `cmake -B build` to Just Work and `find_package`-or-`add_subdirectory` discipline), and Autoware/ROS 2 interop folks (who consume nros as a CMake library inside their colcon workspace). All three groups already have a build tool they trust. Replacing it with `nros build` loses every one of those groups and gains nothing — we don't have Bazel's remote cache, we don't have colcon's `package.xml` graph, and the small team can't maintain a parallel build orchestrator that learns cargo + cmake + west + idf simultaneously.

The Phase 187→195→197 trajectory (just→nros for provisioning) is exactly right. Don't extend it into building. Stop at "provisioning, codegen, metadata, deployment config." Let cargo and cmake do their jobs.

## 7. Implementation deltas vs Phase 212

The Phase 212 proposal — `cargo nros` subcommand + `[workspace.metadata.nros]` in Cargo.toml + `nros-build` build-dep crate — scores **5/5 against the §6 recommendation**. It IS the build-system-native candidate. Keep it.

Specific deltas / sharpenings:

1. **Mirror the design on the cmake side, explicitly.** The proposal is Rust-shaped. Add a sibling: `nano_ros_workspace_metadata(...)` cmake function reading the same fields out of a `cmake/nros.cmake` or top-of-`CMakeLists.txt` block, with `nros-build`-equivalent codegen invocation from cmake. Symmetry matters — C/C++ users should not feel second-class.

2. **`cargo nros` MUST be a wrapper, never a replacement.** Add a hard rule analogous to idf.py `-v`: `cargo nros --explain <verb>` prints the underlying cargo/cmake/cli invocation. Reject any `cargo nros build` verb that doesn't decompose to `cargo nros generate && cargo build`.

3. **`nros-build` build-dep must stay tiny and stable.** It's a build.rs helper, not a framework. If it grows beyond ~500 LOC or pulls in heavy deps, it'll dominate compile time for every consumer crate. Pin scope to: read `[workspace.metadata.nros]`, shell to `nros` CLI, emit `cargo:rerun-if-changed` for the .msg/.srv/.action files it knows about. Nothing else.

4. **`[workspace.metadata.nros]` schema must be a strict subset of `nros-sdk-index.toml` vocabulary.** Don't grow a second TOML dialect. Fields like `domain_id`, `rmw`, `interfaces = [...]`, `platform` already exist conceptually in current `config.toml`/Kconfig; reuse the names.

5. **Reject the orchestrator-driven candidate explicitly in the Phase 212 doc.** Add a "non-goals" section: no `nros build`, no `nros test`, no `nros flash`. Pin the §6 anti-patterns (A1, A4) as the rationale so future phases don't drift.

6. **`just` stays as contributor-side CI orchestration.** Phase 212 should NOT propose retiring `just` in favor of `nros`. The `just ci` / `just test-all` / parallel platform fan-out via Phase 176 jobserver is the right tool for that job and serves the small-team CI use case, not the user-facing API. The migration that ended at Phase 197 (`just` → `nros` for SDK provisioning) is the correct stopping point.

Net: Phase 212 design is sound. Add the cmake sibling, the `--explain` rule, the size budget on `nros-build`, the schema subset rule, the explicit non-goals, and the `just`-stays-for-CI clarification.
