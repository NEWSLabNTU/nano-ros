# Phase 96 — PX4-Autopilot integration

**Goal:** Make `examples/px4/rust/uorb/{talker,listener}` (Phase 90.6)
build and test against a real PX4 SITL binary, end-to-end. PX4-Autopilot
and px4-rs become git submodules under `third-party/px4/`. The SITL
test reuses `px4-sitl-tests`'s `Px4Sitl::boot_in()` fixture for
subprocess management; nano-ros only writes the build-invocation
wrapper.

**Status:** Not Started

**Priority:** P1 (unblocks Phase 90 v1 acceptance)

**Depends on:** Phase 90 v1 (90.1–90.7 landed; 90.7's stubbed
`wait_for_log` will be replaced by the px4-sitl-tests reuse).

---

## Overview

Phase 90 landed nano-ros's PX4 RMW backend + examples + a SITL-test
scaffold. The scaffold has two missing pieces:

1. **PX4-Autopilot tree** — required to build SITL. Today users must
   manually clone it and set `PX4_AUTOPILOT_DIR`.
2. **Working `wait_for_log`** — the Phase 90.7 test stub returns
   "not implemented" instead of actually tailing px4 stdout.

Phase 96 fixes both:

- PX4-Autopilot becomes a git submodule under `third-party/px4/PX4-Autopilot`
  (shallow clone). px4-rs moves from a user-supplied symlink at
  `third-party/px4-rs` to a submodule at `third-party/px4/px4-rs` for
  grouping consistency.
- nros-tests adds `px4-sitl-tests` as a path dev-dependency. The new
  `px4_e2e.rs` calls `Px4Sitl::boot_in(build_dir)` to inherit the heavy
  subprocess infra (drainer thread, line-tail w/ regex, SIGTERM
  process-group cleanup) that px4-rs already wrote and tested.

`just build` is unchanged — PX4 stays opt-in via `just px4 <command>`.

---

## Architecture

```
nano-ros/
├── third-party/
│   └── px4/                              ← new grouping dir
│       ├── PX4-Autopilot/                ← submodule (shallow)
│       │     https://github.com/PX4/PX4-Autopilot.git
│       └── px4-rs/                       ← submodule (was symlink)
│             https://github.com/jerry73204/px4-rs.git
├── packages/
│   ├── px4/
│   │   ├── nros-rmw-uorb/                ← path deps updated
│   │   │     ├── Cargo.toml: third-party/px4/px4-rs/crates/*
│   │   │     └── ...
│   │   └── nros-px4/
│   └── testing/
│       └── nros-tests/
│           ├── Cargo.toml: + px4-sitl-tests path dep
│           └── tests/px4_e2e.rs (rewritten)
└── examples/px4/rust/uorb/                ← path deps updated
    ├── talker/Cargo.toml: third-party/px4/px4-rs/crates/*
    └── listener/...
```

`nros-tests/tests/px4_e2e.rs` flow:

```
test entry
   │
   ▼
build_with_nros_externals()
   │  make -C third-party/px4/PX4-Autopilot px4_sitl_default
   │       EXTERNAL_MODULES_LOCATION=examples/px4/rust/uorb
   │
   ▼  (returns build_dir = third-party/px4/PX4-Autopilot/build/px4_sitl_default)
Px4Sitl::boot_in(&build_dir)
   │  spawn px4 -d etc/init.d-posix/rcS
   │  drainer threads tail stdout/stderr into Mutex<String>
   │  wait for "Startup script returned successfully"
   │
   ▼
sitl.shell("nros_listener start")
sitl.shell("nros_talker start")
   │
   ▼
sitl.wait_for_log(r"recv: ts=\d+ seq=\d+", Duration::from_secs(15))
   │
   ▼
assert_eq!(observed >= 3, true)
   │
   ▼  Drop
SIGTERM process group → 3s grace → SIGKILL
```

---

## Work items

### v1 (96.1–96.9 — required)

- [ ] 96.1 — Add `third-party/px4/PX4-Autopilot` submodule (shallow)
- [ ] 96.2 — Add `third-party/px4/px4-rs` submodule (shallow); remove
      old `third-party/px4-rs` symlink
- [ ] 96.3 — Rewire path deps in `nros-rmw-uorb`, `nros-px4`,
      `examples/px4/rust/uorb/{talker,listener}`
- [ ] 96.4 — Update `just/px4.just`: `setup` initialises both
      submodules; `doctor` verifies both populated + PX4 build deps
- [ ] 96.5 — Update `.env.example`, `.gitignore`, root `Cargo.toml`
      `exclude` list (drop the symlink-specific entries)
- [ ] 96.6 — `just px4 build-sitl` recipe wrapping `make -C
    third-party/px4/PX4-Autopilot px4_sitl_default
    EXTERNAL_MODULES_LOCATION=examples/px4/rust/uorb`
- [ ] 96.7 — Add `px4-sitl-tests` path dev-dep on nros-tests behind
      `px4-sitl` feature
- [ ] 96.8 — Rewrite `nros-tests/tests/px4_e2e.rs` to use
      `Px4Sitl::boot_in()` + `shell()` + `wait_for_log()`. Auto-set
      `PX4_AUTOPILOT_DIR=third-party/px4/PX4-Autopilot` if unset.
      Fail (don't skip) when submodule unpopulated.
- [ ] 96.9 — `just px4 test-sitl` chains `build-sitl` then nextest

### Post-v1 (96.10–96.11)

- [ ] 96.10 — Update `book/src/getting-started/px4.md` with submodule
      setup, `just px4 setup` workflow, full E2E test instructions
- [ ] 96.11 — (Optional) Upstream PR to px4-rs adding
      `Px4Sitl::boot_with_externals(externals_path)` so nano-ros can
      collapse 96.7+96.8 into a single `boot_with_externals(...)` call

---

### 96.1 — PX4-Autopilot submodule

```bash
git submodule add --depth 1 \
    https://github.com/PX4/PX4-Autopilot.git \
    third-party/px4/PX4-Autopilot
git submodule update --init --depth 1 --recommend-shallow \
    third-party/px4/PX4-Autopilot
# PX4-Autopilot has its own ~50 sub-submodules; init recursively shallow:
git -C third-party/px4/PX4-Autopilot submodule update --init \
    --depth 1 --recommend-shallow --recursive
```

Pin to a specific PX4 release tag (e.g. `v1.16.1`) for reproducibility.
Submodule HEAD captured at add time.

**Files:** `.gitmodules`, `third-party/px4/PX4-Autopilot/` (submodule
pointer)

### 96.2 — px4-rs submodule, drop symlink

```bash
# Remove old symlink + .gitignore entry.
rm third-party/px4-rs

git submodule add --depth 1 \
    https://github.com/jerry73204/px4-rs.git \
    third-party/px4/px4-rs
```

Drop `/third-party/px4-rs` from `.gitignore` (no longer a per-user
symlink).

**Files:** `.gitmodules`, `third-party/px4/px4-rs/`, `.gitignore`

### 96.3 — Rewire path deps

Path adjustments — all `third-party/px4-rs/...` → `third-party/px4/px4-rs/...`:

- `packages/px4/nros-rmw-uorb/Cargo.toml` (4 path deps)
- `packages/px4/nros-px4/Cargo.toml` (3 path deps)
- `examples/px4/rust/uorb/talker/Cargo.toml` (6 path deps)
- `examples/px4/rust/uorb/listener/Cargo.toml` (6 path deps)
- `examples/px4/rust/uorb/{talker,listener}/CMakeLists.txt` (PX4_RS_DIR var)

Verify w/ `cargo metadata --no-deps`.

**Files:** all of the above

### 96.4 — `just/px4.just` updates

Replace symlink-management `setup` recipe with submodule init:

```just
setup:
    git submodule update --init --depth 1 --recommend-shallow \
        third-party/px4/PX4-Autopilot \
        third-party/px4/px4-rs
    git -C third-party/px4/PX4-Autopilot submodule update --init \
        --depth 1 --recommend-shallow --recursive
```

`doctor` checks for cmake / ninja / arm-none-eabi-gcc / py3 (PX4 deps),
plus both submodule populated (`third-party/px4/PX4-Autopilot/Tools` +
`third-party/px4/px4-rs/crates` exist).

**Files:** `just/px4.just`

### 96.5 — Workspace housekeeping

- `.env.example`: drop `PX4_RS_DIR` (no longer overridable; fixed
  submodule path). Keep `PX4_AUTOPILOT_DIR` as override for users with
  pre-existing PX4 checkouts.
- `.gitignore`: drop `/third-party/px4-rs` line.
- `Cargo.toml`: update `exclude` entry from `third-party/px4-rs` to
  `third-party/px4/px4-rs` and `third-party/px4/PX4-Autopilot`. PX4-
  Autopilot has no Rust crates but excluding defensively is harmless.

**Files:** `.env.example`, `.gitignore`, `Cargo.toml`

### 96.6 — `just px4 build-sitl`

```just
build-sitl:
    #!/usr/bin/env bash
    set -e
    PX4="$(pwd)/third-party/px4/PX4-Autopilot"
    EXT="$(pwd)/examples/px4/rust/uorb"
    if [ ! -d "$PX4/Tools" ]; then
        echo "ERROR: PX4-Autopilot submodule not populated. Run: just px4 setup"
        exit 1
    fi
    make -C "$PX4" px4_sitl_default \
        EXTERNAL_MODULES_LOCATION="$EXT"
```

Cold build ~10 min; warm <2 s (ninja "no work to do").

**Files:** `just/px4.just`

### 96.7 — `px4-sitl-tests` dev dep

```toml
# packages/testing/nros-tests/Cargo.toml
[dev-dependencies]
px4-sitl-tests = { path = "../../../third-party/px4/px4-rs/tests/sitl", optional = true }

[features]
px4-sitl = ["dep:regex", "dep:px4-sitl-tests"]
```

Note: `px4-sitl-tests` is excluded from px4-rs's workspace (per their
Cargo.toml comment) but is a normal Rust crate that path deps work
on. Their `rust-toolchain.toml` in `tests/sitl/` pins a specific
toolchain — verify it's compatible w/ nano-ros's `tools/rust-toolchain.toml`
in 96.7. If incompatible, document the constraint.

**Files:** `packages/testing/nros-tests/Cargo.toml`

### 96.8 — Rewrite `px4_e2e.rs`

Replace the stubbed `wait_for_log` w/ real px4-sitl-tests calls:

```rust
#![cfg(feature = "px4-sitl")]

use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use px4_sitl_tests::Px4Sitl;
use regex::Regex;

const MIN_MESSAGES: usize = 3;
const RECV_TIMEOUT: Duration = Duration::from_secs(15);

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

fn ensure_px4_autopilot_dir() -> PathBuf {
    if let Ok(dir) = env::var("PX4_AUTOPILOT_DIR") {
        return PathBuf::from(dir);
    }
    // Fall back to vendored submodule.
    let submodule = project_root().join("third-party/px4/PX4-Autopilot");
    assert!(
        submodule.join("Tools").is_dir(),
        "third-party/px4/PX4-Autopilot submodule not populated. \
         Run `just px4 setup` (or set PX4_AUTOPILOT_DIR to your own \
         PX4 checkout)."
    );
    // Set env so px4-sitl-tests sees it.
    unsafe { env::set_var("PX4_AUTOPILOT_DIR", &submodule); }
    submodule
}

fn build_sitl_with_nros_externals() -> PathBuf {
    let px4 = ensure_px4_autopilot_dir();
    let externals = project_root().join("examples/px4/rust/uorb");
    let status = Command::new("make")
        .arg("px4_sitl_default")
        .arg(format!("EXTERNAL_MODULES_LOCATION={}", externals.display()))
        .current_dir(&px4)
        .status()
        .expect("invoke make");
    assert!(status.success(), "PX4 SITL build failed");
    px4.join("build/px4_sitl_default")
}

#[test]
fn px4_sitl_talker_listener_round_trip() {
    let build_dir = build_sitl_with_nros_externals();
    let sitl = Px4Sitl::boot_in(&build_dir).expect("boot");

    sitl.shell("nros_listener start").expect("start listener");
    std::thread::sleep(Duration::from_millis(500));
    sitl.shell("nros_talker start").expect("start talker");

    let pat = Regex::new(r"recv: ts=\d+ seq=\d+ value=").unwrap();
    let line = sitl.wait_for_log(&pat, RECV_TIMEOUT)
        .expect("listener never logged a recv line");
    assert!(line.contains("recv:"));

    // Optional: count multiple lines via successive wait_for_log calls.
}
```

Per CLAUDE.md no-silent-skip rule, `assert!()` is the right shape — test
PANICS if submodule unpopulated, env unset, build fails, or recv
pattern times out.

**Files:** `packages/testing/nros-tests/tests/px4_e2e.rs` (rewrite)

### 96.9 — `just px4 test-sitl`

```just
test-sitl:
    just px4 build-sitl
    cargo nextest run -p nros-tests --features px4-sitl --test px4_e2e
```

Drop the `PX4_AUTOPILOT_DIR` env-var precondition (96.8 auto-resolves
to the submodule).

**Files:** `just/px4.just`

### 96.10 — Doc updates (post-v1)

`book/src/getting-started/px4.md` updated for submodule workflow:

- "Setup" section: `just px4 setup` (clones both submodules)
- Drop the "PX4_RS_DIR" override (no longer applicable)
- "Testing" section: `just px4 test-sitl` builds + runs SITL E2E

`docs/design/px4-rmw-uorb.md` updated to reference px4-sitl-tests reuse
strategy in §10 (zero-copy raw API note → fixture reuse note).

### 96.11 — Upstream PR (optional)

PR `px4-rs#TBD`: add `impl Px4Sitl { pub fn boot_with_externals(externals: &Path) -> Result<Self> { ... } }`. Body invokes `make px4_sitl
EXTERNAL_MODULES_LOCATION=externals` then `boot_in(build_dir)`. Lets
nano-ros's `px4_e2e.rs` collapse to:

```rust
let sitl = Px4Sitl::boot_with_externals(&externals_dir)?;
```

Defer until v1 lands and the integration is proven worth a fork-vs-PR
discussion.

---

## Acceptance criteria

- [ ] `git submodule status` shows both `third-party/px4/PX4-Autopilot`
      and `third-party/px4/px4-rs` populated and pinned.
- [ ] `just px4 setup` succeeds on a fresh checkout (cold submodule
      init).
- [ ] `just px4 doctor` passes when PX4 build deps installed.
- [ ] `cargo metadata --no-deps` clean — all path deps resolve.
- [ ] `cargo check --workspace` clean.
- [ ] `cargo test -p nros-rmw-uorb --features 'std test-helpers'` all
      7 host-mock tests still green.
- [ ] `just px4 build-sitl` produces
      `third-party/px4/PX4-Autopilot/build/px4_sitl_default/bin/px4`.
- [ ] `just px4 test-sitl` runs the full E2E test on a machine w/ PX4
      build prereqs. Test passes (listener observes ≥1 `recv:` line).
- [ ] `just build` chain unchanged (does NOT touch PX4).
- [ ] `just ci` chain unchanged (does NOT run SITL test).

---

## Notes

- **Submodule weight:** PX4-Autopilot full clone is ~3 GB w/
  sub-submodules. Shallow clone trims to ~500 MB. Most contributors
  who don't need PX4 work skip `just px4 setup` entirely; submodules
  uninit'd by default after `git clone nano-ros`.
- **Shared build cache:** PX4's ninja caches based on
  `EXTERNAL_MODULES_LOCATION` content hash + source mtimes. Switching
  between two `EXTERNAL_MODULES_LOCATION` values triggers full rebuild
  each time. Stick to nano-ros's `examples/px4/rust/uorb` only.
- **px4-rs toolchain pin** (`tests/sitl/rust-toolchain.toml`) may
  conflict with nano-ros's pinned nightly. If so, either:
  - Pin both projects to the same nightly (preferred — small change in
    px4-rs's `tests/sitl/rust-toolchain.toml`)
  - Document the constraint and let the user manage two toolchains
- **CI runtime:** Cold SITL build adds ~10 min to test run. Default
  CI omits `px4-sitl` feature; dedicated `px4-ci` job opts in.
  Nightly CI worth doing once SITL test is stable.
- **PX4-Autopilot Python deps:** `Tools/setup/ubuntu.sh` installs apt
  packages w/ sudo. Per CLAUDE.md project rule, nano-ros never invokes
  sudo. Document deps; user runs setup manually if missing. `just px4
doctor` reports which deps are absent.

## Risks

- **Submodule storage cost** for contributors who don't care about
  PX4. Mitigated by shallow clones + opt-in init.
- **PX4 release tag drift** — pinning a specific PX4 tag means
  nano-ros lags PX4 main. Update tag periodically (or post-v1, every
  PX4 release).
- **px4-rs's `Px4Sitl` API is internal-ish** (their `tests/sitl/` is a
  test crate, not a library crate). Upstream may refactor without
  semver compat. Mitigated by submodule pin + 96.11 PR for stable
  surface.

## Prerequisites checklist (verify before starting)

- [ ] Phase 90 v1 (90.1–90.7) committed
- [ ] User has tested: `cargo test -p nros-rmw-uorb --features 'std test-helpers'` → 7 passing
- [ ] User has working PX4 build env: cmake, ninja, gcc, python3 + PX4
      pip packages (`pyros-genmsg`, `pymavlink`, etc.)
