# Phase 336 — Build-profile propagation (one knob per language, no literals)

**Status (2026-08-07).** W1–W7 landed; W7's verification sweep closed it out on
2026-08-05. Explicitly NOT done here: the per-board size table (the carve-outs
mean the embedded numbers move independently) — see the W7 note below.

**Touches:** RFC-0042 (platform & build determinism — the profile is now part of
what "determinism" covers). **Related:** RFC-0026 (fixture/example shape — the
profile is in every artifact path), issue 0023 / 0024 (size classes that a
profile default can break).

**Sequencing with phase-334 (build-cache layout).** Both phases rewrite artifact
paths: 334 moves WHERE a target dir lives, this phase changes WHICH profile
segment appears inside it. They do not conflict — the profile segment is derived
from `nros profile dir` at every consumer here, so wherever 334 relocates a
cache root, the segment follows. If 334 lands first, W3/W4 read the relocated
roots unchanged; if this lands first, 334 inherits one resolver instead of three
literals.

## Goal

A user picks the optimization level **once, in their own build system** —
`CMAKE_BUILD_TYPE` for C/C++, a cargo profile for Rust — and nano-ros propagates
that choice through every crate it builds on their behalf. No file inside
nano-ros names a profile literally, and no user workspace has to learn an
`nros-*` name to build.

## Why now

`just ci` on a fresh clone runs `cargo build --release` in ~15 places. Today's
`[profile.release]` is `opt-level = "s"` + `lto = "fat"` + `codegen-units = 1` —
the slowest build the toolchain can produce, chosen for image size. The fast
profile the repo already has (`nros-fast-release`) reaches only two paths:
`scripts/build/cargo.sh` (fixtures) and the codegen tool. Everything routed
through CMake/Corrosion — `nros-c`, `nros-cpp`, the workspace umbrella, the
`cpp_ffi` glue crate — silently gets `--release` because no
`corrosion_import_crate` call passes `PROFILE`, so Corrosion falls back to its
own `CMAKE_BUILD_TYPE` mapping.

Two problems, one cause:

1. **Users cannot express intent.** `-DCMAKE_BUILD_TYPE=Debug` still produces a
   fat-LTO cargo build of every nano-ros crate. The C/C++ and Rust halves of the
   same image disagree about what was asked for.
2. **The mapping is spelled three times.** `profile → target subdirectory` lives
   in bash (`scripts/build/cargo.sh:60`), Rust
   (`packages/testing/nros-tests/src/fixtures/binaries/mod.rs:639`), and a just
   literal (`just/qemu-baremetal.just:15`); `profile → cargo flags` twice more;
   and `NanoRosGenerateInterfaces.cmake:421,424` hardcodes `/release/` in the
   FFI artifact path. This is the drift class CLAUDE.md names: fix it as a class
   with ONE shared helper, not a fourth spelling.

## Design contract

### Two knobs, not one

C/C++ and Rust have different option vocabularies, so they keep their own knob.
`CMAKE_BUILD_TYPE` is untouched and propagates exactly as CMake always does.
Rust gets `NROS_CARGO_PROFILE` (CMake cache variable, or environment for the
non-CMake lanes). When it is unset, it is **derived** from `CMAKE_BUILD_TYPE`:

| `CMAKE_BUILD_TYPE` | cargo profile | settings |
| --- | --- | --- |
| `Debug` | `dev` | unchanged (opt-level 1, per repo) |
| `RelWithDebInfo` | `nros-relwithdebinfo` | opt 2, debug 1, lto off, cgu 16, incremental |
| `MinSizeRel` | `nros-minsizerel` | opt `"s"`, lto fat, cgu 1, panic abort |
| `Release` | `release` | opt 3, lto fat, cgu 1, panic abort |
| unset / empty | `nros-relwithdebinfo` | the development default |
| any other value | — | fatal error naming `NROS_CARGO_PROFILE` |

Corrosion's own default is `Debug → dev`, **everything else → release**. That is
what this phase replaces: it maps a `-O0`-intent CMake build onto a fat-LTO
cargo build, which is not a defensible reading of the user's request.

`[profile.release]` changes meaning: it becomes the **performance** profile
(opt 3), and today's size semantics move to `nros-minsizerel` unchanged. Every
current `--release` site therefore has to be repointed deliberately — see W5.

### Ownership rule: who defines a profile

The profile NAME decides who owns its definition:

- **Name starts with `nros-`** → nano-ros owns it. We pass `--profile <name>`
  **and** inject the definition through the environment:
  `CARGO_PROFILE_NROS_MINSIZEREL_INHERITS=release`,
  `CARGO_PROFILE_NROS_MINSIZEREL_OPT_LEVEL=s`, … A user workspace needs no
  `[profile.*]` block at all.
- **Any other name** (`dev`, `release`, or the user's own) → we pass the name
  only and inject nothing. The user's manifest governs; if the profile is
  undefined, cargo's own `error: profile '<name>' is not defined` is the right
  error and points at the right file.

This is what makes "the user maintains their own profiles" and "a CMake build
type can select an `nros-*` profile" hold at the same time.

**Verified empirically** (2026-08-04, cargo 1.x on this host), because both
halves of the rule depend on it:

```console
$ cargo build --profile nros-minsizerel                    # bare crate, no preset
error: profile `nros-minsizerel` is not defined

$ CARGO_PROFILE_NROS_MINSIZEREL_INHERITS=release \
  CARGO_PROFILE_NROS_MINSIZEREL_OPT_LEVEL=s \
  cargo build --profile nros-minsizerel
    Finished `nros-minsizerel` profile [optimized] target(s)
```

and the precedence that forces the rule to be name-scoped rather than blanket:

```console
$ # manifest says opt-level = 1
$ CARGO_PROFILE_NROS_MINSIZEREL_OPT_LEVEL=3 cargo build --profile nros-minsizerel -v
... -C opt-level=3      # environment WINS over the manifest
```

Blanket injection would therefore silently override a user's own definition.
Scoping injection to the `nros-` namespace is what prevents that.

### SSoT: `nros profile`

The table above is data, and it is consumed from bash, cmake, just, and Rust. It
lives in the CLI so there is exactly one implementation:

```
nros profile resolve --build-type RelWithDebInfo   # -> nros-relwithdebinfo
nros profile args    nros-minsizerel               # -> --profile nros-minsizerel
nros profile dir     nros-minsizerel               # -> nros-minsizerel   (dev -> debug)
nros profile env     nros-minsizerel               # -> CARGO_PROFILE_NROS_MINSIZEREL_INHERITS=release
                                                   #    CARGO_PROFILE_NROS_MINSIZEREL_OPT_LEVEL=s ...
```

`nros profile env` prints nothing for a non-`nros-` name — the ownership rule,
mechanized once. `nros-tests` links the crate directly (no subprocess); bash and
cmake shell out via the existing tool-path plumbing.

No bootstrap cycle: building the CLI itself only ever uses cargo's built-in
`dev` / `release`, so `scripts/bootstrap.sh` never needs `nros profile`.

### Resulting shape

```
        user's choice                    nano-ros internals
   ─────────────────────────      ────────────────────────────────────
   CMAKE_BUILD_TYPE ──────────────► C/C++ flags        (cmake, unchanged)
          │
          │ nros profile resolve
          ▼
   NROS_CARGO_PROFILE ────┬───────► corrosion_import_crate(PROFILE …)   × 5
   (cache var / env,      │             nros-c · nros-cpp · umbrella
    overrides the map)    │             riscv64 board · zenoh staticlib
                          ├───────► corrosion_set_env_vars(CARGO_PROFILE_*)
                          ├───────► _nros_ffi_cargo_args  (cpp_ffi glue)
                          ├───────► NROS_CODEGEN_CARGO_PROFILE (codegen crates)
                          ├───────► scripts/build/cargo.sh  (fixtures, just lanes)
                          └───────► nros_profile::dir()     (test-side paths)
```

### User usage

Pure-CMake workspace — nothing added to any `Cargo.toml`:

```bash
cmake -B build -DCMAKE_BUILD_TYPE=MinSizeRel   # C/C++: -Os ; Rust: --profile nros-minsizerel
cmake --build build                             # ...definition supplied via environment
```

Split the two sides — small C/C++, debuggable Rust:

```bash
cmake -B build -DCMAKE_BUILD_TYPE=MinSizeRel -DNROS_CARGO_PROFILE=nros-relwithdebinfo
```

The user's own profile — they own the definition, nano-ros injects nothing:

```toml
# user-workspace/Cargo.toml
[profile.prod]
inherits = "release"
opt-level = 3
lto = "fat"
```

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release -DNROS_CARGO_PROFILE=prod
```

Pure-cargo workspace — no configuration at all; nano-ros crates are path deps,
so the workspace-root profile already governs them:

```bash
cargo build --profile prod
```

nano-ros development — same ergonomics, faster default:

```bash
just ci                                          # nros-relwithdebinfo
NROS_CARGO_PROFILE=nros-minsizerel just zephyr   # size run
```

## Work items

### W1 — `nros profile` (the SSoT)

- [ ] `nros-cli-core`: a `profile` module holding the preset table, the
      `CMAKE_BUILD_TYPE` map, the flag/dir derivations, and the env-injection
      emitter. Public Rust API (`resolve`, `args`, `dir`, `env`) so `nros-tests`
      links it instead of re-implementing.
- [ ] `nros profile {resolve,args,dir,env}` subcommand over that API. Unknown
      build type → non-zero exit + a message naming `NROS_CARGO_PROFILE`.
- [ ] Unit tests for every row of the table, both directions, plus: `env` is
      empty for non-`nros-` names; `dir` maps `dev → debug` and is identity
      otherwise.

**Acceptance:** `cargo test -p nros-cli-core profile::` green; the four verbs
print the table's values.

### W2 — Presets

- [ ] Root `Cargo.toml`: add `[profile.nros-relwithdebinfo]` (renamed from
      `nros-fast-release`) and `[profile.nros-minsizerel]` (today's `release`
      settings verbatim); redefine `[profile.release]` as opt 3.
- [ ] Rename the ~6 in-repo `nros-fast-release` references (`.cargo/config.toml`,
      `justfile:36`, `just/qemu-baremetal.just`, `cargo.sh`, `binaries/mod.rs`,
      `NanoRosRuntimeCrate.cmake`). No alias is kept — the name is internal.
- [ ] Delete the mirrored `[profile.*]` blocks from the generated umbrella
      (`NanoRosRuntimeCrate.cmake:134`) and `cmake/cpp_ffi_Cargo.toml.in:15`;
      env injection replaces them. Two of four mirrors gone.

**Acceptance:** `cargo build --profile nros-minsizerel -p nros-core` produces
byte-identical output to today's `--release` build of the same crate (proves the
settings moved, not changed).

### W3 — Consumers stop re-implementing the table

- [ ] `scripts/build/cargo.sh`: `nros_cargo_profile_args` /
      `nros_cargo_target_profile_dir` / `nros_cargo_nextest_args` /
      `nros_cargo_profile_arg_string` all delegate to `nros profile`.
- [ ] `fixtures/binaries/mod.rs`: `cargo_profile_name` / `cargo_target_profile_dir`
      / `cargo_build_args` call the `nros-cli-core` API.
- [ ] `just/qemu-baremetal.just:15`: drop the literal `nros-fast-release`
      fallback — the resolver is the fallback.

**Acceptance:** `git grep -nE '"(dev|release)" =>|debug\)$'` finds no
profile→dir mapping outside the CLI crate.

### W4 — CMake propagation

- [ ] `NROS_CARGO_PROFILE` cache variable, defaulted by
      `nros profile resolve --build-type ${CMAKE_BUILD_TYPE}` at configure time;
      unknown type is a `FATAL_ERROR`.
- [ ] All five `corrosion_import_crate` sites gain `PROFILE ${NROS_CARGO_PROFILE}`:
      `packages/api/nros-c/CMakeLists.txt:61`, `packages/api/nros-cpp/CMakeLists.txt`,
      `cmake/NanoRosRuntimeCrate.cmake:268`,
      `cmake/board/nano-ros-board-riscv64-qemu.cmake:489`,
      `packages/rmw/zenoh/nros-rmw-zenoh-staticlib/CMakeLists.txt`.
- [ ] `corrosion_set_env_vars(<target> ${_nros_profile_env})` on each, from
      `nros profile env`.
- [ ] `_nros_ffi_cargo_args PROFILE release` → the knob; the two `/release/`
      literals in `NanoRosGenerateInterfaces.cmake:421,424` → `nros profile dir`.

**Acceptance:** a workspace fixture configured `-DCMAKE_BUILD_TYPE=Debug` shows
`--profile dev` in its cargo command line, and one configured `MinSizeRel` shows
`--profile nros-minsizerel`; a user-side workspace with no `[profile.*]` block
builds under both.

### W5 — The hand-rolled `--release` sites

- [ ] Route through `cargo.sh`: `just/freertos.just:{103,158,326,396}`,
      `just/nuttx.just:483`, `just/threadx-linux.just:220`,
      `just/threadx-riscv64.just:344`, `justfile:1559`, and the artifact paths
      that hardcode `/release/` beside them.
- [ ] Preserve the carve-outs, with their reasons re-stated at the new site:
      NuttX forces `nros-minsizerel`-class optimization because the image
      miscompiles at opt 2 (`workspace-fixtures-build.sh:199`, never
      root-caused — phase-285 W5); fixture #126 (mps2 zenoh-pico) stays
      optimized because `-O0` misses QEMU timing.
- [ ] CLI/tool builds (`bootstrap.sh`, `justfile:3023`, `setup-launch-resolve`)
      keep plain `--release` — they are host tools outside the propagation graph,
      and keeping them literal is what avoids the bootstrap cycle.

**Acceptance:** `git grep -n -- '--release' just/ scripts/build/` returns only
the host-tool builds, each with a comment saying why.

### W6 — Gate + docs

- [ ] `scripts/check-build-profile-literals.sh`: fails if a profile name or a
      `target/**/release/` artifact path appears outside the CLI crate, the
      preset definitions, and the allow-listed host-tool builds. Wire into
      `check-fast` (buildless, source-free).
- [ ] Book: a `build-profiles.md` page under `user-guide/` carrying the table
      and the four usage shapes above; link it from the workspace and
      installation pages.
- [ ] `docs/reference/platform-implementation-notes.md`: the NuttX opt-2
      miscompile and the mps2 timing floor become named profile constraints.

**Acceptance:** `just check fast` runs the gate; the gate fails on a planted
literal.

### W7 — Verification sweep

> **W7 landed 2026-08-05.** What the sweep actually cost is worth recording,
> because almost none of it was profile propagation.
>
> **Two regressions in this phase's own work, both found by running it:**
> - The generated C++ FFI glue is a SECOND Rust staticlib in a link that already
>   holds `libnros_cpp.a`. At `lto = "off"` both carry std's panicking codegen
>   unit → `multiple definition of __rustc::rust_begin_unwind`, five workspaces
>   dead. The hardcoded `--release` had been supplying fat LTO silently; it is
>   now the `cpp-ffi-glue` carve-out. **A hardcoded value can be load-bearing
>   for a reason nobody wrote down — "it built before" is evidence about the old
>   profile's SETTINGS, not about the site.**
> - `NROS_CARGO_PROFILE := ""` (W3, so the table owns the default) met
>   `env::var`, which returns `Ok("")` for a set-but-empty variable. The profile
>   name became `""`, every fixture path became `target//<bin>`, and 110 tests
>   reported "not prebuilt" — a plausible-but-wrong message that sent the first
>   diagnosis at stale fixtures. Shell and cmake already treated empty as unset;
>   the rule is now `nros_cargo_profile::profile_or_default`, tested.
>
> **Five pre-existing breaks on main**, none caused here, all blocking
> verification: the workspace-fixture sweep passing `''` args (0406 made `--id`
> a flag, the generator stayed positional), `check-dep-chain` not syncing the
> leaf configs that were untracked as `nros sync` output, two CLI tests reading
> SystemModels that phase-330 W4 deleted, the `model_location` ladder test not
> tracking two rungs W4/W7 added (and a poisoned test mutex reporting one
> failure as five), and two gates walking the filesystem for TRACKED files.
> Four more tests of the same W4 class were fixed upstream in parallel
> (`2994d5a46`, issue #414) and arrived on the final rebase.
>
> **Performance, since the sweep spent most of its wall-clock waiting:** the
> compile-check stage walked ~27 independent rows serially — 5 rows in 10
> minutes with ONE rustc on 32 cores. Now fanned out under pinned make 4.4's
> FIFO jobserver (`scripts/build/jobserver-pool.sh`, extracted rather than
> spelled a third time): 696s for all 26. The size probe rebuilt a ~60-crate
> graph per consumer per build dir (~420 MB each) — one shared cache, 63s → 16s.
> The workspace-fixture entry lookup walked 24k files per row and could pick a
> cargo artifact over the cmake one; the metadata probe linked one binary per
> component serially, 24s → 4s.
>
> **Not done here:** the per-board size table. The carve-outs mean the embedded
> images are byte-unchanged (both resolve to `nros-minsizerel`, which carries
> `release`'s pre-split settings), so the delta that motivated the table is
> zero for every board that has one; measuring it needs a tier-3 sweep this
> phase does not otherwise require.


- [ ] Rebuild every fixture lane — the profile is in the artifact path, so all
      prebuilt fixtures are invalid on landing (`just build-test-fixtures
      lane=<lane>` per lane).
- [ ] `just ci-matrix` (tier 2: the diff touches `cmake/` and `packages/core`).
- [ ] Size delta report: `nros-sizes-build` before/after for every board. The
      new default (`opt 2`, no LTO) makes embedded images **larger** than
      today's hardcoded `--release`. Any board that overflows pins
      `nros-minsizerel` in its own board module, and the pin is recorded here.

**Acceptance:** tier 2 green; no board over its flash budget; the size table is
committed with the phase.

## Risks

**Embedded images grow.** This is the deliberate cost of a development-speed
default. Issue 0024 (ESP32 DRAM overflow) is exactly this failure mode, and
issue 0023 records that the size probe itself is LTO-sensitive. W7 measures
before anything is called done; a board that cannot afford the default pins
`nros-minsizerel` rather than the default being re-litigated.

**`release` changes meaning.** Anyone with muscle memory for
`cargo build --release` in this tree now gets opt 3 instead of `opt-level = "s"`.
The gate in W6 catches new literals; the rename in W2 catches existing ones.
Out-of-tree scripts are not covered — the book page in W6 is the mitigation.

**Corrosion version floor.** `PROFILE` requires rust ≥ 1.57 and a Corrosion that
supports custom profiles; the pinned `0.5.1-nros1` does
(`share/cmake/Corrosion.cmake:922,965`). A user on an older Corrosion from their
own `CMAKE_PREFIX_PATH` gets Corrosion's own error, not ours.

## Non-goals

- Unifying the C/C++ and Rust knobs into one variable. They have different
  option vocabularies; one knob would have to invent a lossy middle language.
- Generating `[profile.*]` blocks into user workspaces. Env injection makes it
  unnecessary, and writing into a user's manifest would fight the ownership
  rule.
- Root-causing the NuttX opt-2 miscompile. It stays a carve-out with a pointer
  to phase-285 W5.
