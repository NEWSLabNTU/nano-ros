# Build System & Caching

> **Audience.** Contributors working on the repo build/test matrix. End users
> never need this — they consume nano-ros with a single
> [`add_subdirectory(<repo-root>)`](../getting-started/build-as-subdirectory.md),
> no install step, no `find_package`. This handbook documents the **other**
> build world: the in-repo `just` orchestration that builds + tests every
> platform × RMW, and the caching that keeps it incremental and correct.
>
> Canonical sources this page summarises: `CLAUDE.md` ("Build", "Build tiers",
> "Build parallelism"), Phase 176 (unified jobserver), Phase 181 (fixture SSOT
> + Ninja), Phase 177.9 (staleness probes).

## TL;DR cheat-sheet

```bash
just setup                 # one-time SDK/toolchain install (tiered; see CLAUDE.md)
direnv allow               # once after clone — else zpico-sys/build.rs panics

just build                 # workspace + transports (fast inner loop)
just build-examples        # + every example
just build-all             # + test fixtures (everything test-all consumes)
just build-test-fixtures   # just the fixtures + writes the test-all stamp

just test-unit             # ~5s          ⊂
just test-integration      # ~30s         ⊂
just test                  #              ⊂
just test-all              # heavy QEMU/Zephyr/ROS-interop/miri/codegen

just <plat> build|build-all|test|ci   # narrow to one platform first
NROS_BUILD_JOBS=8 just build-all       # cap parallelism
```

**Rule of thumb:** a platform-specific failure → run the narrow
`just <plat> build-all` (or closest `build`/`build-examples`/`build-fixtures`)
*before* the root `just build-all`. Always `just ci` after a task. Never `sudo`
— if a step needs it, tell the user.

## Generators — Ninja by default

The repo's CMake builds use **Ninja** (`-G Ninja`), falling back to Make only
when `ninja` is absent. Ninja gives reliable incremental rebuilds and fits the
Phase 176 fifo jobserver; Make was dropped from the staleness path because
`make -q` mis-reported up-to-dateness.

- Configure-once: `scripts/build/cmake-incremental.sh::nros_cmake_configure_if_needed`
  configures a build dir only when its `CMakeCache.txt` / generated build system
  is missing; `cmake --build` then handles reconfigure on `CMakeLists` changes.
- Generator-mismatch wipe: a dir configured with the *other* generator is
  `rm -rf`'d and reconfigured (you can't switch generators in place).
- Pinned tools: the unified jobserver needs `make ≥ 4.4` + `ninja ≥ 1.13`
  (apt's 4.3/1.10 lack the fifo jobserver). `just workspace install-make` /
  `install-ninja` build them into `third-party/{make,ninja}`; `.envrc` puts them
  on `PATH` (incl. a `gmake` → make-4.4 alias).

## Per-RMW build dirs = cache isolation

Each example builds into a **per-RMW** directory with its **own** cargo target
dir:

```
examples/<plat>/<lang>/<example>/
  build-zenoh/       cargo/   …   # -DNROS_RMW=zenoh
  build-xrce/        cargo/   …   # -DNROS_RMW=xrce
  build-cyclonedds/  cargo/   …   # -DNROS_RMW=cyclonedds
```

Because each RMW (and its Corrosion cargo target dir) is physically separate,
selecting a different RMW **cannot** collide with another RMW's cache — there is
no shared target dir to invalidate. Platform is fixed per build dir
(`-DNANO_ROS_PLATFORM=<plat>` at configure). This layout is why the old manual
cache-invalidation idea (archived Phase 145) was retired: the directory shape
makes it unnecessary. (Rust-only examples build via plain `cargo` into
`target-<rmw>/`; the per-RMW principle is the same.)

## Parallelism — `NROS_BUILD_JOBS` + the unified jobserver

One knob scales every parallel recipe:

```bash
NROS_BUILD_JOBS=8 just build-all     # default: nproc
```

**Unified jobserver (Phase 176).** `just build-all` auto-routes to
`build-all-jobserver` when the pinned `make 4.4` + `ninja 1.13` are present: a
single GNU-make fifo jobserver spans cargo + build-script `cc` + ninja-via-west
+ cmake, allocating tokens dynamically instead of a static per-tool split. Under
the jobserver (`NROS_JOBSERVER=1`) recipes drop their explicit
`-j`/`--parallel`/`CMAKE_BUILD_PARALLEL_LEVEL` so the tools inherit the pool.

- Same artifacts either way; without the pinned tools it falls back to a static
  split.
- `NROS_NO_JOBSERVER=1` forces the static path.
- Never re-introduce a hardcoded `parallel --jobs <n>` without threading
  `${NROS_BUILD_JOBS:-N}` through.

See `docs/roadmap/archived/phase-176-unified-jobserver-build-orchestration.md`.

## Build & test tiers

Each tier is a strict superset of the previous:

| Build | contains |
|-------|----------|
| `build` | workspace + transports |
| `build-examples` | ⊃ build + every example |
| `build-all` | ⊃ build-examples + test fixtures |

| Test | wall-clock | contains |
|------|-----------|----------|
| `test-unit` | ~5 s | unit |
| `test-integration` | ~30 s | ⊃ + integration |
| `test` | — | ⊃ |
| `test-all` | minutes | ⊃ + heavy QEMU/Zephyr/ROS-interop + doc + miri + C codegen |

Per-platform: `just <plat> build ⊂ build-examples ⊂ build-fixtures ⊂ build-all`
and `just <plat> test|test-all|ci`. `<plat>` = target families
(`qemu`, `zephyr`, board groups). Support services (`zenohd`, `cyclonedds`) are
**not** platform scopes. Orchestration lives in `justfile` + `just/*.just`.

## Fixture SSOT — `examples/fixtures.toml`

Per-fixture build options (features, `--target-dir`, env, per-RMW variants,
cmake `-D` defs, cross target) live in **one manifest**,
[`examples/fixtures.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/fixtures.toml),
consumed by **both** the build recipes and the test-all staleness probe — so the
build and the probe use identical options (no feature-thrash).

- Reader: `scripts/build/fixtures-manifest.py list --platform <p> --lang <l> [--rmw <r>] [--for-probe]`.
  Emits `\x1f`-separated records (unit-separator, not tab — tab is IFS-whitespace).
- Builder: `scripts/build/fixtures-build.sh <plat> [lang] [rmw]` — one shared loop;
  rust cells go through `cargo build`, C/C++ cells through `cmake --build`
  (configure-once + per-RMW dir). Platform-wide `-D` defs (toolchain, codegen
  tool, SDK dirs) are injected by the recipe via `NROS_CMAKE_EXTRA_DEFS`.
- Cross-toolchain platforms inject `RUSTUP_TOOLCHAIN` / SDK env via the recipe
  (e.g. ESP32-C3 = riscv32imc on the workspace nightly + build-std).

## Staleness discipline

A prebuilt fixture that's silently stale would let `test-all` run against the
wrong binary. The discipline:

- **Presence gate.** `build-test-fixtures` stamps `target/nextest/.fixtures-built`;
  `test-all`'s `_require-fixtures` fast-fails (~1 s) with a hint if it's absent
  (bypass: `NROS_SKIP_FIXTURE_CHECK=1`).
- **Rust cells.** `scripts/test/rust-fixture-stale.sh` reuses cargo's own
  fingerprint — `cargo build … --message-format=json` reports `"fresh":false`
  for a stale unit, so the probe both detects **and** self-heals.
- **C/C++ cells.** `scripts/test/cmake-fixture-stale.sh` runs the incremental
  `cmake --build` (near-no-op when fresh) and flags cells that actually rebuilt.
  (`ninja -n` is unusable here — Corrosion's cargo step is an always-run custom
  command, so it always reports pending.)
- **Probe opt-out.** Cells needing a recipe-injected toolchain the probe can't
  supply (e.g. the ESP32 nightly + build-std) set `skip_probe = true`; the reader's
  `--for-probe` omits them so they don't toolchain-thrash.
- **Source-list / drift gates.** `zpico-sys` (vendored zenoh-pico, Phase 136.6)
  and `nros-rmw-xrce-cffi` (vendored uxr/micro-cdr, Phase 145.4) verify each
  vendored source root resolves to a real dir with `.c` files and panic with a
  `git submodule update --init` hint on drift. cbindgen build scripts
  (`nros-c`, `nros-cpp`, `zpico-sys`) emit `cargo:rerun-if-changed=cbindgen.toml`
  + `src/`.
- **Probe runs in preflight.** `_check-fixtures-stale` runs both probes (rust +
  cmake) over the manifest before the `test-all` nextest stage; it warns +
  self-heals rather than hard-failing.

## Patched QEMU

QEMU networked tests use a patched `qemu-system-arm` (icount + MPS2 fixes) built
by `just qemu setup-qemu` into `build/qemu/bin/`. The harness picks it up
automatically (`nros_tests::qemu::qemu_system_arm_cmd()`); the system binary is
the fallback. See [Patched qemu-system-arm](./qemu-patched-binary.md).

## See also

- [Build Commands](../reference/build-commands.md) — the user-facing quick reference.
- [zpico-sys Build Architecture](./zpico-build.md) — the zenoh-pico cross-compile.
- `CLAUDE.md` — the authoritative build/test-tier + parallelism policy.
- Phase docs (`docs/roadmap/archived/`): 176 (jobserver), 181 (fixture SSOT +
  Ninja), 177.9 (staleness probes).
