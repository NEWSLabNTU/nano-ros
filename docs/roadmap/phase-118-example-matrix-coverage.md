# Phase 118: Example Matrix — Collapse Per-RMW Dirs + Matrix Lint

**Goal:** Replace the per-RMW directory axis
(`examples/<plat>/<lang>/<rmw>/<case>/`) with a single
`examples/<plat>/<lang>/<case>/` shape where the RMW is chosen at
build time via a Cargo feature (Rust) or a cmake `-D` arg
(C / C++). Test harness builds the same example under each
supported RMW with isolated `--target-dir` / `-B build-<rmw>`.
Result: one source-of-truth per (platform, language, case), ~30
duplicated example dirs collapsed, RMW becomes a build-system
flag instead of a directory.

**Status:** Not Started.

**Priority:** Medium — quality-of-life + maintenance reduction.
Per-RMW dirs duplicate ~95% of the source body (Cargo dep + 1
register call + locator default are the only differences). Every
new feature added to "talker" today fans out across N RMW dirs.

**Depends on:** Phase 128 (Cargo-manifest / `target_link_libraries`
RMW selection — landed), Phase 129 (platform-aliases + multi-RMW
bridge mode — landed). The build-time selection mechanism is
already in place; this phase wires it into the example tree.

**Supersedes:** the prior Phase 118 "fill every (plat × lang ×
rmw × case) cell" scope. After the collapse the matrix axis
`<rmw>` becomes a build matrix (test fixture toggle) rather than
a directory axis, so the fill target shrinks from ~120 missing
example crates to ~10–20 missing (plat × lang × case) cells. The
documented out-of-scope cells (118.E.1 bare-metal C/C++, 118.E.2
px4) carry over unchanged.

---

## Overview

### Current shape

```
examples/<plat>/<lang>/<rmw>/<case>/
    Cargo.toml          # nros-rmw-<rmw> dep + features
    src/main.rs         # nros_rmw_<rmw>::register() + locator default
    .cargo/config.toml
    package.xml         # name = "native-<rmw>-<case>"
    CMakeLists.txt      # set(NANO_ROS_RMW <rmw>)
```

Per-RMW directories duplicate the entire example body. Diff
between `native/rust/zenoh/talker` and `native/rust/dds/talker`
is:

- `Cargo.toml` — one `nros-rmw-<X>` dep line + features list
- `src/main.rs` — one `nros_rmw_<X>::register()?` line + locator
  default string
- `package.xml` — name string
- `.gitignore` — same boilerplate
- everything else byte-identical

For ~10 (plat × lang) cells × 3 RMWs = 30 dirs duplicate this
diff. Adding a new feature to "talker" requires edits across all
matching cells.

### Target shape

```
examples/<plat>/<lang>/<case>/
    Cargo.toml          # optional rmw deps gated by features
    src/main.rs         # #[cfg(feature = "rmw-X")] register blocks
    .cargo/config.toml
    package.xml
    CMakeLists.txt      # nano_ros_link_rmw(... RMW ${NROS_RMW})
```

RMW selection moves to the build invocation:

- **Rust:** `cargo build --no-default-features --features rmw-dds`
- **C / C++:** `cmake -B build -S . -DNROS_RMW=dds`
- **No source change** to switch RMW.

The matrix lint walks `examples/` and confirms every `(plat,
lang, case)` cell carries the canonical six cases (talker,
listener, service-{client,server}, action-{client,server}) and
exposes the RMW build-matrix as `Cargo.toml` `[features]` keys
plus the cmake `NANO_ROS_RMW` option set.

### Conflict avoidance

Phase 128 already established two RMWs can't be linked into the
same Cargo unit simultaneously without bridge-mode opt-in (Phase
129); each RMW pulls in mutually exclusive platform features.
The collapse avoids that by:

- **Per-RMW `--target-dir`** — same pattern as Phase 88 zero-copy
  / safety-e2e variants documented in `CLAUDE.md`'s "Parallel
  build isolation" note. `target-zenoh/`, `target-dds/`,
  `target-xrce/` are gitignored under the example's per-dir
  `.gitignore`.
- **Optional Cargo deps** — each `nros-rmw-*` is `optional =
  true`; the matching `rmw-<X>` feature gates the dep. Only the
  selected RMW's rlib enters the dep graph.
- **No `Cargo.lock` clobber** — Cargo writes the lockfile inside
  the active `--target-dir` (Cargo's default behaviour) so
  separate dirs hold separate locks. Source-tree `Cargo.lock`
  matches the default feature.
- **cmake per-RMW `-B build-<rmw>`** — analogous isolation for
  C / C++ builds. The `nano_ros_link_rmw(... RMW ${NROS_RMW})`
  function emits the strong `nros_app_register_backends()` stub
  into the active build dir.

---

## Architecture

### A. Cargo feature scaffold

```toml
# examples/<plat>/<lang>/<case>/Cargo.toml
[package]
name = "<plat>-<lang>-<case>"
edition = "2024"

[features]
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh"]
rmw-dds   = ["dep:nros-rmw-dds"]
rmw-xrce  = ["dep:nros-rmw-xrce-cffi"]

[dependencies]
nros            = { path = "../../../../packages/core/nros" }
nros-rmw-zenoh  = { path = "../../../../packages/zpico/nros-rmw-zenoh",
                    features = ["std", "platform-<plat>", "ros-humble"],
                    optional = true }
nros-rmw-dds    = { path = "../../../../packages/dds/nros-rmw-dds",
                    features = ["platform-<plat>"],
                    optional = true }
nros-rmw-xrce-cffi = { path = "../../../../packages/xrce/nros-rmw-xrce-cffi",
                    optional = true }
```

The exact `features = […]` per RMW dep is the per-platform tweak
that Phase 128.D folded into each example today — preserved
per-cell.

### B. `src/main.rs` RMW-agnostic shape

```rust
use nros::{Executor, ExecutorConfig};

#[cfg(feature = "rmw-zenoh")]
const DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";
#[cfg(feature = "rmw-dds")]
const DEFAULT_LOCATOR: &str = ""; // brokerless
#[cfg(feature = "rmw-xrce")]
const DEFAULT_LOCATOR: &str = "127.0.0.1:2019";

fn register_active_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register")?; }
    #[cfg(feature = "rmw-dds")]
    { nros_rmw_dds::register().map_err(|_| "dds register")?; }
    #[cfg(feature = "rmw-xrce")]
    { nros_rmw_xrce_cffi::register().map_err(|_| "xrce register")?; }
    Ok(())
}

fn main() {
    register_active_rmw().expect("rmw register");
    let config = ExecutorConfig::new(
        std::env::var("NROS_LOCATOR").as_deref().unwrap_or(DEFAULT_LOCATOR),
    );
    let mut executor = Executor::open(&config).expect("open");
    /* … rest of the case (talker / listener / service / action) … */
}
```

`#[cfg]` chains are mutually exclusive at the feature level
because at most one `rmw-*` feature is active per build. The
default `rmw-zenoh` makes `cargo run` Just Work for the
docs-first experience.

### C. cmake glue

`examples/<plat>/<lang>/<case>/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.16)
project(<plat>-<lang>-<case> LANGUAGES C CXX)

set(NANO_ROS_PLATFORM <plat>)
set(NANO_ROS_RMW "${NROS_RMW}" CACHE STRING "Active RMW (zenoh|dds|xrce|cyclonedds)")

add_subdirectory(${CMAKE_CURRENT_LIST_DIR}/../../../.. nano_ros)
add_executable(<case> src/main.<c|cpp>)
target_link_libraries(<case> PRIVATE NanoRos::NanoRos)
nros_platform_link_app(<case>)
nano_ros_link_rmw(<case> RMW ${NANO_ROS_RMW})
```

`NROS_RMW` defaults to `zenoh` (matches Rust default feature) so
out-of-the-box `cmake -B build && cmake --build build` produces
the canonical experience.

### D. Test harness build matrix

`packages/testing/nros-tests/src/fixtures/binaries.rs` already
runs per-example cargo / cmake builds. Extend to accept an RMW
parameter:

```rust
pub fn build_rust_example(case: &str, rmw: Rmw) -> PathBuf {
    let target_dir = format!("target-{}", rmw.feature_suffix()); // "zenoh" / "dds" / "xrce"
    Command::new("cargo")
        .args(["build", "--release",
               "--no-default-features",
               "--features", &rmw.cargo_feature(),
               "--target-dir", &target_dir])
        .current_dir(...)
        .status()...
}
```

`tests/rtos_e2e.rs` / `tests/native_api.rs` parametrize over the
allowed RMWs per cell (the matrix lint exposes which RMWs are
compiled in for each example).

### E. Matrix snapshot script

`tools/example-matrix.py` walks `examples/`, reads each
`Cargo.toml` `[features]` block to learn which RMWs are
`optional` for that cell, and prints:

```
platform                 lang  case               zenoh dds xrce
native                   rust  talker             Y     Y   Y
native                   rust  listener           Y     Y   Y
native                   rust  service-server     Y     Y   Y
…
```

Plus a "Deliberately empty" subsection for the documented holes
(bare-metal C/C++, px4 row). `--lint` mode exits non-zero on:

- A `(plat, lang, case)` cell present in `examples/` whose
  `Cargo.toml` doesn't expose any `rmw-*` feature.
- A `(plat, lang, case)` cell absent from `examples/` without
  being named in `examples/README.md`'s "Deliberately empty"
  subsection.

---

## Work Items

### Tier 1 — Mechanism PoC

- [ ] **118.A.1 — Collapse PoC on `native/rust/talker/`.** Take
      the three sibling dirs (`zenoh`, `dds`, `xrce`) under
      `native/rust/`, copy `zenoh/talker/` to `native/rust/talker/`,
      merge the Cargo.toml feature scaffold (B), rewrite main.rs
      to the cfg-dispatched shape (B). Build under each feature
      with isolated `--target-dir`. Confirm `cargo build
      --features rmw-{zenoh,dds,xrce}` all succeed.

- [ ] **118.A.2 — Test-harness extension.** Add
      `binaries::build_rust_example_rmw(case, rmw)` returning the
      per-RMW binary path. Update one `native_api.rs` test
      (pubsub-zenoh and pubsub-dds) to use it. Verify both still
      green.

- [ ] **118.A.3 — cmake glue draft on `native/c/talker/`.**
      Single CMakeLists.txt with `nano_ros_link_rmw(... RMW
      ${NROS_RMW})`. Build under `-DNROS_RMW=zenoh` and
      `-DNROS_RMW=dds`, confirm both produce working talker bins.

### Tier 2 — Roll-out per platform

Each item collapses the per-RMW dirs under one (plat × lang)
cell into a single per-case dir, and deletes the old per-RMW
dirs after the matching test harness updates pass:

- [ ] **118.B.1 — `native/rust/`.** Collapse zenoh + dds + xrce
      siblings for all six cases.
- [ ] **118.B.2 — `native/c/`.** Same.
- [ ] **118.B.3 — `native/cpp/`.** Same.
- [ ] **118.B.4 — `qemu-arm-freertos/{c,cpp,rust}/`.** Same.
- [ ] **118.B.5 — `qemu-arm-nuttx/{c,cpp,rust}/`.** Same.
- [ ] **118.B.6 — `qemu-riscv64-threadx/{c,cpp,rust}/`.** Same.
- [ ] **118.B.7 — `threadx-linux/{c,cpp,rust}/`.** Same.
- [ ] **118.B.8 — `zephyr/{c,cpp,rust}/`.** Same (Zephyr west
      build needs a sibling cmake change to pass `NROS_RMW`
      through to the integration shell).
- [ ] **118.B.9 — `qemu-arm-baremetal/rust/`.** Zenoh + DDS
      siblings only (no XRCE on bare-metal).
- [ ] **118.B.10 — `qemu-esp32-baremetal/rust/`.** Same.
- [ ] **118.B.11 — `esp32/rust/`.** Zenoh-only today; the
      collapse is a no-op until a second RMW lands on ESP32, but
      restructure the dir into the new `<case>/` shape so future
      RMW work doesn't recreate the per-RMW axis.
- [ ] **118.B.12 — `stm32f4/rust/`.** Same as ESP32 — zenoh-only,
      restructure-only.

### Tier 3 — Matrix lint + docs

- [ ] **118.C.1 — `tools/example-matrix.py`.** Walks
      `examples/`, prints the cell × RMW matrix (E). `--lint`
      flag for CI.
- [ ] **118.C.2 — `examples/README.md`.** Autogenerated table +
      "Deliberately empty" subsection (carries the existing
      118.E.1 / E.2 docs forward, retitled to the new schema).
- [ ] **118.C.3 — `just check-example-matrix`.** Wraps the
      script's `--lint` mode. Wired into `just ci`.
- [ ] **118.C.4 — `nros_tests::matrix` integration test.**
      Drives the same `--lint` from nextest; fails CI on
      untriaged cells.

### Tier 4 — Out-of-scope documentation

(Carried forward from the prior Phase 118 — items already done.)

- [x] **118.D.1 — Bare-metal C/C++ holes documented**
      (`examples/README.md` "Intentionally empty cells",
      Phase 118.E.1 from the original scope, 2026-05-17).
- [x] **118.D.2 — `px4/{c,rust}` holes documented**
      (Phase 118.E.2 from the original scope, 2026-05-17).

### Tier 5 — Cleanup

- [ ] **118.E.1 — Delete legacy `<plat>/<lang>/<rmw>/`
      directories** after every Tier 2 item lands. Per-RMW
      subdirs go away; per-case dirs are the canonical shape.
- [ ] **118.E.2 — Update justfile build-fixtures recipes** to
      iterate `(case, rmw)` tuples per platform. Each case
      builds against every RMW its `Cargo.toml` features
      declare available (so platforms where DDS isn't viable
      just don't expose `rmw-dds` in the feature list).
- [ ] **118.E.3 — CLAUDE.md "Examples = Standalone Projects"
      update.** Replace the `<plat>/<lang>/<rmw>/<case>/`
      canonical-shape pointer with the new `<plat>/<lang>/<case>/`
      shape + a one-line note on the RMW feature/cmake-arg
      mechanism. Phase 131's "canonical example shape" rule
      gets a matching revision.

---

## Files

```
tools/example-matrix.py                            (new, 118.C.1)
examples/README.md                                 (rewritten, 118.C.2 + 118.D.x)
examples/<plat>/<lang>/<case>/Cargo.toml           (one per cell, 118.B.x)
examples/<plat>/<lang>/<case>/src/main.{rs,c,cpp}  (RMW-agnostic, 118.B.x)
examples/<plat>/<lang>/<case>/CMakeLists.txt       (C/C++ cells, 118.B.x)
packages/testing/nros-tests/src/fixtures/binaries.rs   (extended, 118.A.2)
packages/testing/nros-tests/tests/example_matrix.rs    (new, 118.C.4)
justfile                                           (118.C.3 + 118.E.2)
CLAUDE.md                                          (118.E.3)
```

---

## Acceptance criteria

- [ ] Every `examples/<plat>/<lang>/<case>/Cargo.toml` exposes
      `rmw-*` features for the RMWs that compile on that
      platform; no per-RMW directory remains.
- [ ] `cargo build --no-default-features --features rmw-X` builds
      every Rust example under `--target-dir target-X/` for every
      `X` listed in the example's `Cargo.toml`.
- [ ] `cmake -B build-X -DNROS_RMW=X` configures every C / C++
      example for every supported RMW.
- [ ] `tools/example-matrix.py --lint` exits 0 on `main` and the
      `nros_tests::matrix` test passes in CI.
- [ ] `just test-all` is bit-identical in pass / fail set to the
      pre-collapse baseline (no regression — the collapse is a
      refactor, not a feature change).
- [ ] `examples/README.md` carries the autogenerated table and
      the "Deliberately empty" subsection, regenerated whenever
      a cell is added or removed.
- [ ] CLAUDE.md "Examples = Standalone Projects" reflects the new
      shape; Phase 131's canonical-shape rule is amended in step.

---

## Notes

- **Why optional Cargo deps over per-feature build profiles?**
  Optional deps + features compose naturally with Cargo's
  feature-unification rules. Profiles (`[profile.dev-zenoh]`)
  don't gate dependency graph membership, so a `zenoh` profile
  would still drag dds + xrce into the dep graph unless every
  dep is `optional`. Once they're `optional`, profiles add no
  extra value over `--features`.

- **Why `--target-dir target-<rmw>/` instead of one
  `target/` with feature-keyed subdirs?** Cargo writes its
  fingerprint database keyed on the workspace's `target/`
  hash — switching `--features` invalidates everything else
  in `target/`, forcing a full rebuild every time the test
  harness flips RMWs. Per-RMW `--target-dir` preserves
  incremental state across RMW switches. Same reason CLAUDE.md
  already requires this for the safety-e2e and zero-copy
  variants.

- **What about `cargo install` / `cargo run` ergonomics?**
  The default `rmw-zenoh` feature keeps `cargo run` in an
  example dir working out of the box — same one-line
  `cargo run -p <example>` experience users have today.
  Power users override with `--features` when they want a
  non-default RMW.

- **Why `nano_ros_link_rmw()` already does the C / C++ side
  cleanly?** Phase 144.5.c established `add_subdirectory(<repo>)`
  consumption + `nano_ros_link_rmw()` strong-stub emission as
  the canonical C / C++ entry point. The cmake-side collapse
  is pre-staged — only the per-example `set(NANO_ROS_RMW ...)`
  line changes to read `NROS_RMW` from a `-D` arg instead of
  being hardcoded per-dir.
