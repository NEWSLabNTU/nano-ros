# Phase 262 — decouple nros-macros from nros-cli-core (leaf-crate extraction)

Status: **Planned (2026-06-18)** · Resolves
[issue 0083](../issues/0083-nros-macros-build-deps-on-cli-core.md) · Removes the
build-coupling root behind the phase-253 docs/check-c lane breakages.

> **Goal.** `nros-macros` (the proc-macro every nros app uses) pulls ALL of
> `nros-cli-core` (planner / codegen / orchestration / SDK store + the
> `ros-launch-manifest-types` submodule) just to call two self-contained modules:
> `pkg_index` (workspace walk) and `launch_parser` (launch.xml parser). Extract
> those two into leaf crates so the macro — and therefore every `nros` / `nros-c` /
> example / fixture build — stops compiling nros-cli-core and stops needing the
> submodule.

## Investigation (2026-06-18) — the modules are clean

Dependency chain today:
```
nros-c → nros → nros-macros (proc-macro)
              → nros-build → nros-cli-core → ros-launch-manifest-types (submodule)
                                          → planner / codegen / orchestration / sdk
```
- `nros-macros` uses ONLY `nros_build::pkg_index::{detect_workspace_root,
  build_pkg_index}` + `nros_build::launch_parser::parse_launch_file` (re-exports of
  `nros_cli_core::{pkg_index, launch_parser}`).
- `pkg_index.rs` — self-contained leaf (eyre, quick_xml, serde, walkdir; no
  `crate::`/`super::`).
- `launch_parser.rs` — leaf + only `crate::pkg_index::PkgIndex`; **does NOT use
  `ros-launch-manifest-types`**.
- `ros-launch-manifest-types` (`ln_types`) is used ONLY in
  `orchestration/manifest.rs` (the planner) — incidental to the macro path.
- Depending on `nros-build` compiles all of it (emit.rs → `orchestration::plan` +
  `codegen::entry` → nros-cli-core), which is why the macro drags the whole CLI in.

So: the submodule requirement on the macro path is purely incidental; neither
module the macro needs touches it. No feature-gating required.

## Work items

### W1 — `nros-pkg-index` leaf crate — DONE (2026-06-18)
New crate `packages/cli/nros-pkg-index/` ← `pkg_index.rs` (git mv). Deps: eyre,
quick-xml, serde, **serde_json** (used full-path for the index cache), walkdir.
nros-cli-core re-exports it (`pub use nros_pkg_index as pkg_index`) — consumers +
`crate::pkg_index` internal refs (launch_parser) unchanged. Verified: leaf +
nros-cli-core + nros-build compile; nros-build's 8 `pkg_index` tests green.

### W2 — `nros-launch-parser` leaf crate — DONE (2026-06-18)
New crate `packages/cli/nros-launch-parser/` ← `launch_parser.rs` (git mv). Deps:
eyre, quick-xml, nros-pkg-index (rewrote `crate::pkg_index::PkgIndex` →
`nros_pkg_index::PkgIndex`). nros-cli-core re-exports it (`pub use
nros_launch_parser as launch_parser`). Verified: leaf + nros-cli-core + nros-build
compile; nros-build's 10 `launch_parser` tests green. (No serde/ln_types — clean.)

### W3 — rewire nros-cli-core (keep consumers byte-stable) — DONE (W1/W2)
Done incrementally in W1+W2: nros-cli-core deletes both modules, deps the two leaf
crates, re-exports `pub use nros_pkg_index as pkg_index` + `…nros_launch_parser as
launch_parser`. Keeps `ros-launch-manifest-types` (manifest.rs uses it). All
nros-cli-core + nros-build consumers/tests unchanged + green.
Delete the two modules from nros-cli-core; depend on the two leaf crates;
re-export `pub use nros_pkg_index as pkg_index;` + `pub use nros_launch_parser as
launch_parser;`. Every existing `nros_cli_core::{pkg_index,launch_parser}` user
(incl nros-build's re-export + tests) keeps compiling unchanged. nros-cli-core
KEEPS `ros-launch-manifest-types` (manifest.rs still uses it — the CLI legitimately
needs it).

### W4 — swap nros-macros off nros-build — DONE (2026-06-18)
nros-macros Cargo.toml: dropped `nros-build`; added `nros-pkg-index` +
`nros-launch-parser`. main_macro.rs: `nros_build::pkg_index::*` → `nros_pkg_index::*`,
`nros_build::launch_parser::*` → `nros_launch_parser::*` (the only three uses).
**Verified:** `cargo tree -p nros-c` now shows NO nros-cli-core / nros-build /
ros-launch-manifest-types — only the two leaf crates. nros-macros builds.

### W5 — remove the CI submodule-init workarounds
With the macro path submodule-free, the `ros-launch-manifest` `git submodule
update --init` steps added for the coupling can go:
- `docs.yml` "Init CLI launch-manifest submodule" (book rustdoc-driver builds nros).
- The build tier still inits it for the actual CLI build (nros-cli-core) — keep
  that one; only the macro-driven lanes (docs, and any plain `cargo build -p
  nros-c`) no longer need it. Verify check-c (in check-build) still has the
  submodule via the CLI-build step. Re-confirm a plain `cargo build -p nros-c`
  succeeds with NO submodule initialised.

## Acceptance
- `cargo build -p nros-c` (and any `nros`-dependent app) succeeds on a checkout
  with the `ros-launch-manifest` submodule ABSENT.
- `cargo tree -p nros-c` shows no `nros-cli-core` / `ros-launch-manifest-types`.
- `just ci` green; nros-cli-core + nros-build tests unchanged + green.
- docs.yml no longer needs the submodule-init step.

## Notes / placement
Leaf crates land in `packages/cli/` (codegen-support, sub-workspace members);
nros-macros path-deps across the workspace boundary as it already does for
nros-build (lockfile-grouped path dep — fine for a proc-macro, host-only at the
consumer's compile time). Extraction is mechanical (self-contained modules); main
care is the nros-cli-core re-export shim so no downstream import path changes.
