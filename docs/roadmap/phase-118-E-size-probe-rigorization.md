# Phase 118.E — Size-Probe Rigorization

**Goal:** Replace the parallel-build retry loop in `nros-sizes-build::find_dep_rlib` with a deterministic mechanism. Investigate cbindgen const propagation as the leading candidate; document findings; pick an actionable path.
**Status:** 118.E.1 + .5 + .6-prep landed (probe-mode dispatch, env-var contract, unit tests, doc updates, llvm-nm portability fixes). Filesystem mode remains default; `isolated` opt-in works end-to-end. .2/.3/.4 pending.
**Priority:** Medium (build reliability; current retry loop works but flakes under parallel + slow filesystems).
**Depends on:** Phase 118.B (rlib probe canonical, hand-math deleted), Phase 118.C (retry loop introduced as stopgap).

## Overview

`nros-c` and `nros-cpp` build scripts depend on target-correct values of
`size_of::<Executor>()`, `size_of::<RmwPublisher>()`, etc. to emit
`#define NROS_EXECUTOR_SIZE N` in the generated C/C++ FFI headers.
Today, this is done by:

1. `nros::sizes::export_size!` emits a `#[used] static __NROS_SIZE_FOO: [u8; size_of::<T>()]` whose **symbol storage size** in the rlib equals `size_of::<T>()`.
2. `nros-sizes-build::find_dep_rlib` scans `<target>/<triple>/<profile>/deps/` for `libnros-<hash>.rlib`.
3. `extract_sizes` walks the rlib's `ar` members, reads ELF/Mach-O/COFF symbol sizes (or falls back to `llvm-nm` on bitcode under fat LTO).

Phase 118.C added a **15-second retry loop** because, under parallel
workspace builds (the common case), `nros-c`'s build script can fire
before sibling `nros`'s target-triple rlib has finished writing. The
loop polls the filesystem at 200 ms ticks. It works but is intrinsically
fragile: timeout depends on machine speed; deeper parallelism widens
the race window; no signal of completion vs. genuinely missing.

This phase audits a rigorous replacement.

## Architecture

### Forces

- **Target-correct sizes are mandatory.** Types contain `*mut`, `&[u8]`, `extern "C" fn`, `usize` — all pointer-size-dependent. Host evaluation produces wrong values when cross-compiling to embedded targets (4-byte ptrs vs. 8-byte host ptrs).
- **Cargo provides no "barrier" primitive.** A build script cannot declare "wait for sibling crate's target-triple lib build to finish." Build scripts compile for the host; `[build-dependencies]` is host-only. The only sequencing primitive is `links = "..."` + emitted metadata, which itself runs at host build-script time and cannot measure target sizes.
- **The data is in the rlib.** Post-compile, the rlib contains target-correct sizes as symbol storage or as v0-mangled const-generic monomorphisations. Reading it is correct; *when* to read is the question.

### Option matrix

| # | Mechanism | Race? | Future-proof | Cost | Verdict |
|---|-----------|-------|--------------|------|---------|
| 1 | Current: filesystem-poll retry loop | Mitigated (15s timeout) | Low — depends on `target/<triple>/<profile>/deps/` layout, which cargo may sandbox per-crate (rust-lang/cargo discussions) | Zero | Stopgap |
| 2 | cbindgen const propagation | None (compile-time eval) | High (parser-level) | Investigation | **Infeasible** — see Decision |
| 3 | Nested `cargo build -p nros --target=$TARGET --message-format=json` | None (synchronous artifact event) | High (stable JSON schema) | One nested cargo invocation per consumer build script; lockfile contention | **Recommended** |
| 4 | `cargo rustc -Zprint-type-sizes` parse | None | Low — unstable, output format churns | Nightly-only | Reject |
| 5 | Read rmeta directly via internal API | None | Low — rmeta format is unstable | Brittle | Reject |
| 6 | `nros-node` emits sizes via `cargo:metadata` (DEP_NROS_NODE_*) | None | High | Build scripts run for **host** — wrong sizes when target ≠ host | Infeasible |
| 7 | Two-stage external build (just-driven sequencing) | None | High | Imposes ordering on every caller; breaks `cargo build` workflow | Reject |

### Decision: cbindgen const propagation is infeasible

cbindgen is an AST-level tool (uses `syn`). It can emit:

- Literal-valued consts: `pub const FOO: u32 = 42;` → `#define FOO 42`.
- Cfg-conditional values it can statically resolve.
- Item shapes from dependencies via `parse.parse_deps = true` + `parse.include = [...]`.

It **cannot** evaluate:

- `core::mem::size_of::<T>()` — type-layout call, requires post-monomorphisation rustc knowledge that cbindgen does not have.
- Const fn calls whose body inspects type layout (e.g. the project's `const fn u64s_for<T>() -> usize { ... }`).

Evidence already in-tree: `packages/core/nros-c/src/opaque_sizes.rs:25`
defines

```rust
pub const SESSION_OPAQUE_U64S: usize = u64s_for::<nros::internals::RmwSession>();
```

and `packages/core/nros-c/cbindgen.toml` `[export] exclude = [...]`
lists `SESSION_OPAQUE_U64S`, `PUBLISHER_OPAQUE_U64S`,
`GUARD_HANDLE_OPAQUE_U64S`, `EXECUTOR_OPAQUE_U64S`,
`NROS_LIFECYCLE_CTX_OPAQUE_U64S` with the rationale "the value is a
`u64s_for::<T>()` expression cbindgen can't evaluate". This is the same
shape as `nros::sizes::EXECUTOR_SIZE`. Propagating consts via cbindgen
would hit the same wall.

`[parse.expand]` (cbindgen's `cargo expand` mode) does **not** rescue
this. `cargo expand` runs `rustc -Zunpretty=expanded`, which expands
declarative and procedural macros into the post-macro AST but does not
perform const evaluation. `size_of::<T>()` is a function call, not a
macro, and survives expansion unchanged. Verified by inspection of
`rustc -Zunpretty=expanded` output; also confirmed indirectly by the
fact that the project's `*_OPAQUE_U64S` consts wouldn't need exclusion
if `[parse.expand]` resolved them.

### Recommendation: Nested cargo + `--message-format=json`

Replace the filesystem-poll loop in `find_dep_rlib` with a synchronous
nested cargo invocation:

```text
cargo build -p nros --target=$TARGET --features=$ACTIVE_FEATURES \
            --message-format=json --release
```

Parse stdout for `{"reason":"compiler-artifact","target":{"name":"nros",...},"filenames":["..."]}` events. The first event for `target.name = "nros"`, `target.kind = ["lib"]` yields the canonical rlib path. The invocation only returns after rustc finishes writing the rlib, eliminating the race.

#### Concerns and mitigations

1. **Lockfile contention — IMPORTANT correction.** Cargo workspaces hold an exclusive flock on `<target>/.cargo-lock` for the **entire** outer build, including the time spent waiting on build-script subprocesses. A nested `cargo build` from a build script targeting the **same** dir deadlocks. The earlier draft of this doc mis-stated this — corrosion/cargo-c/cargo-make invoke nested cargo from CMake/Make at a layer *outside* any in-progress cargo build, not from a build script.
   - **Mitigation actually used in implementation:** the nested invocation uses a **separate** target dir (`$OUT_DIR/sizes-probe-target/` by default; overridable via `NROS_SIZES_PROBE_TARGET_DIR`). This sidesteps the flock at the cost of a duplicate compile of the probed crate on cold cache. Subsequent runs hit the probe dir's own cache and are fast (<1s).
   - Empirically verified on x86_64-linux: full `just build-all` passes under both `NROS_SIZES_PROBE_MODE=filesystem` (default) and `NROS_SIZES_PROBE_MODE=isolated`. Generated `#define NROS_*_SIZE` values are bit-identical between the two modes.

2. **Feature set propagation.** The nested invocation must build `nros` with the *same* feature set the outer build is resolving. Build scripts receive features as `CARGO_FEATURE_<NAME>=1` env vars but only for the *current* crate's features. The active feature set for `nros` is recoverable via `cargo metadata --filter-platform=$TARGET --format-version=1` and walking the resolve tree.
   - Cost: one `cargo metadata` call before the nested `cargo build`.

3. **Speed.** Nested `cargo build` on an already-built tree is fast (cargo's fingerprint cache hits, no re-codegen). Incremental cost is sub-second on cached builds. Cold builds are dominated by `nros` compile time, which the outer build was going to do anyway.

4. **Recursion.** `nros-c`'s build script must not transitively trigger `nros-c`'s own rebuild. `cargo build -p nros` (with explicit `-p`) confines the build to `nros` and its deps; `nros-c` is not in `nros`'s dep tree. Safe.

5. **`cargo` binary path.** Build scripts receive `CARGO` env var pointing at the active cargo binary. Use it instead of bare `cargo` to honour rustup overrides.

#### Migration steps

1. Add `cargo_run_json` helper in `nros-sizes-build` that spawns `cargo build -p <name> --target=<triple> --message-format=json [--features=<list>]`, streams stdout, returns the first matching `compiler-artifact.filenames[0]`.
2. Add `nros_sizes_build::resolve_active_features(crate_name)` using `cargo metadata --filter-platform`.
3. Refactor `find_dep_rlib` to call (1) and (2); delete the retry loop and the `OUT_DIR`-walking fallback.
4. Keep `extract_sizes` unchanged.
5. Gate behind an env override `NROS_SIZES_PROBE_MODE=nested|filesystem` for one release cycle; default `nested`. Allow rollback during the transition.
6. Drop the `filesystem` mode after one release if no regressions.

### Long-term: cargo upstream feature request

The fundamental issue is cargo's lack of a primitive for "my build
script depends on completion of a sibling crate's target-triple
library build." Filing this as a cargo RFC would benefit all build-
script-driven codegen (cxx, cbindgen, autocxx, bindgen-with-types).
Track as a stretch goal; not blocking.

## Work Items

### 118.E.1 — Prototype nested-cargo probe — **DONE**

- **Files:** `packages/core/nros-sizes-build/src/lib.rs`.
- [x] Add `ProbeMode::{Filesystem,Isolated}` enum + `from_env` reading `NROS_SIZES_PROBE_MODE` (case-insensitive; default `filesystem` for safety during transition).
- [x] Refactor `find_dep_rlib` to dispatch on `ProbeMode`.
- [x] Implement `find_dep_rlib_isolated`: spawn `cargo build -p <crate> --target=<triple> [--release] --features=<...> --message-format=json-render-diagnostics` against `$OUT_DIR/sizes-probe-target/`. Parse `compiler-artifact` events; return the first `.rlib` filename whose `target.name` matches.
- [x] Implement `forwarded_features()` that reverses cargo's `CARGO_FEATURE_<NAME>=1` upper-case-with-underscore transform back to the lowercase-with-dashes form expected by `--features`.
- [x] Make filesystem-path timeout configurable via `NROS_SIZES_PROBE_TIMEOUT_SECS` (default 60s, up from a hard-coded 10s+5s) with progress `cargo:warning=` every 10s.
- [x] Fix `extract_sizes_via_llvm_nm` portability: `llvm-nm` → `llvm-nm.exe` on Windows; `rustc_host_triple()` actually returns the *host* triple (was returning `TARGET`, which broke cross-builds because `lib/rustlib/<target>/bin/` doesn't carry the host toolchain).
- [x] Unit tests for `ProbeMode::from_env` and `forwarded_features`.
- [x] End-to-end smoke: `just build-all` passes (EXIT=0) under default (`filesystem`) mode; `cargo build -p nros-c --features rmw-zenoh,platform-posix,ros-humble` passes under both modes with identical generated sizes.

### 118.E.2 — Active-feature resolution

- **Files:** `packages/core/nros-sizes-build/src/lib.rs`.
- Use `cargo metadata --filter-platform=<TARGET>` to derive the active feature set for `nros` from the consumer's perspective. Cache result via `OUT_DIR/sizes-meta.json` keyed by `Cargo.lock` mtime.

### 118.E.3 — Cross-compile validation

- **Files:** `packages/core/nros-c/build.rs`, `packages/core/nros-cpp/build.rs`.
- Verify nested probe produces correct sizes for `thumbv7em-none-eabi`, `riscv32imac-unknown-none-elf`, `aarch64-unknown-linux-gnu` cross-builds. Add a `just check-size-probe` recipe that builds all three and asserts emitted `#define`s differ between 32- and 64-bit targets (pointer-size sanity check).

### 118.E.4 — Lockfile-contention soak test

- **Files:** `packages/testing/nros-tests/tests/size_probe_concurrency.rs` (new).
- Spawn `cargo build --workspace -j 16` ten times in a row under a CI runner; assert no deadlocks, no panics, all sizes match.

### 118.E.5 — Documentation + rollout

- **Files:** `docs/reference/size-probe.md` (new), `CLAUDE.md`.
- Document the probe mechanism end-to-end: producer macro, nested invocation, future-stability surface.
- Update CLAUDE.md's Phase 118 entry to flip the size-probe story from "retry loop" to "nested cargo, deterministic."

### 118.E.6 — Retire the filesystem path

- **Files:** `packages/core/nros-sizes-build/src/lib.rs`.
- Remove the retry loop, the `OUT_DIR`-walking `cargo_target_dir`, and the `NROS_SIZES_PROBE_MODE` env knob.
- Bump `nros-sizes-build` major version.

## Acceptance

- [ ] 118.E.1 lands; both probe modes selectable via env var; defaults to `nested`.
- [ ] 118.E.2 lands; feature resolution matches `cargo build` output for all three RMW backends.
- [ ] 118.E.3 passes for the three listed cross-targets.
- [ ] 118.E.4 soak test: 10× workspace build, no deadlocks, no probe failures.
- [ ] 118.E.5 docs landed.
- [ ] 118.E.6 filesystem path removed; CI green; no `NROS_SIZES_PROBE_MODE=filesystem` references in tree.

## Notes

### Why not move all sizes to literal consts?

Tempting: hand-write `pub const EXECUTOR_SIZE: usize = 248;` per target.
Rejected because:

- Sizes drift silently when fields are added to `Executor`, `RmwSession`, etc. The current `size_of::<T>()` is the single source of truth and self-updating.
- Per-target literal tables would need maintenance across `thumbv7em`, `thumbv7m`, `riscv32imac`, `riscv32imc`, `x86_64`, `aarch64`, ESP32 Xtensa, etc. Combinatorial with RMW backend feature set.
- Catches no real bug — the probe already enforces the truth.

### Why not embed sizes in `nros`'s emitted C header instead?

Considered: have `nros` itself (not `nros-c`/`nros-cpp`) own the
header emission, so the sizes are read from itself. Rejected because:

- `nros` is the runtime crate; it has no business emitting C codegen. Separation of concerns says codegen lives in `nros-c` / `nros-cpp`.
- Still doesn't solve the race — the codegen still needs to run *after* `nros`'s lib compile, and build scripts run in parallel with dep compilation.

### Fat-LTO bitcode fallback stays

Even with deterministic nested-cargo invocation, the rlib's object
members may be LLVM bitcode if upstream sets `lto = "fat"`. The
existing `llvm-nm`-based v0 demangling fallback in
`extract_sizes_via_llvm_nm` is unaffected by this phase and remains
necessary. The portability gaps in that fallback (Windows `.exe`
suffix, cross-target `llvm-nm` lookup using `TARGET` instead of host
triple) are tracked separately under Phase 118.F.

### Future cargo per-crate target sandboxing

If cargo eventually adopts per-crate target sandboxing
(rust-lang/cargo#5931 family), the filesystem-scan path dies entirely.
The nested-cargo + `--message-format=json` path survives because the
artifact event reports the sandbox-resolved path explicitly.
