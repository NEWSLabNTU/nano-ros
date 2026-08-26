# phase-383 — `nros build`, the colcon-like builder (implement RFC-0065)

**Implements:** [RFC-0065](../design/0065-colcon-like-workspace-builder.md) (D1–D14)
**Amends in passing:** [RFC-0024](../design/0024-multi-node-workspace-layout.md) §2.4/§9,
[RFC-0026](../design/0026-example-directory-layout.md) (copied-out *workspaces*),
[RFC-0070](../design/0070-build-cache-layout.md) R1 (`install/` → `dist/`)
**Closes:** issue 0798 (the (entry × board) pairing becomes derived)
**Touches:** [RFC-0063](../design/0063-system-model-is-a-build-artifact.md)/phase-330
(who owns `build/`), [RFC-0077](../design/0077-image-runtime-is-the-images-choice.md)
(the `panic` policy this forwards)

**Status (2026-08-26). NOT STARTED — waves defined, no code written.**

## Goal

A nano-ros workspace user authors three things — node code, the launch tree,
and per-board overlays — and types **one command**. The workspace root build
file and the entry packages both stop being hand-written, because both are
derived from `(launch, args, board)`.

Today's user types six commands and maintains nine root build files plus
fifteen entry packages in one workspace alone.

## The ordering constraint that matters

**The builder ships and proves itself BEFORE anything migrates, and the old
paths are retired only after the migration lands** (RFC-0065 D13).

> W1–W7 build it · W8 proves it on hostile trees · W9 migrates the nine ·
> W10 retires the old paths.

Migrating during development means re-migrating on every design change.
Retiring before migrating strands the tree. Never retiring is how a project
ends up with two ways to build — the thing this phase exists to remove.

## Global constraints

Every wave inherits these; they are not restated per task.

| constraint | value | source |
| --- | --- | --- |
| CMake floor | **3.22** — `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>` (3.24+) is unavailable | measured on this host |
| Stage 5 | **`execvp`** — never a pipe, never `Command::output()` | RFC-0065 D1; the RFC-0024 §2.4 amendment depends on it |
| Flag vocabulary | **no `--target`** — the word already means four things here and users pass cargo's through | RFC-0065 D7 |
| `panic` values | `platform` \| `halt` \| `own` — the **existing** RFC-0077 enum, forwarded | RFC-0077 |
| Generated output | deterministic: no timestamps, no absolute paths, stable ordering | reproducibility (W3.c gates it) |
| Build trees | `build/<coord>/`, coordinate in RFC-0070 R2's vocabulary — **never a new suffix** | RFC-0070 R2 |
| Cargo invocation | one target dir per workspace, never per package | RFC-0070's 80.6 GB measurement |

## What the simulation already found

RFC-0065's *Migration simulation* walked the nine in-tree workspaces plus
`autoware-safety-island` (branch `nano-ros`) and `nano-ros-rt-eval`. Ten
findings, F1–F10, are requirements on the waves below rather than discoveries
to be made during them. Each wave names the ones it carries.

**These two downstream projects are the acceptance bar** (W8). Our own
workspaces are uniform by construction and will not surface F4–F6.

**One decision deliberately has no task.** RFC-0065 **D11** ("a custom board is
a board crate, not an overlay") is a boundary statement, not work: it says where
vendor-generated board *source* belongs and defers the authoring story to
RFC-0012. W7.e/W7.f give that crate its force-link and linker-fragment seams;
nothing else is owed.

---

## W1 — The image declaration (BLOCKS EVERYTHING)

Carries **F3, F7, F8**. Nothing else can start: every later wave reads this
schema.

- [ ] **W1.a** Add `[image.<id>]` (RFC-0065 D6) to the system-config schema in
      `packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`,
      beside the existing `DeployTargetMetadata`. Fields: `kind`
      (`self`|`embedded`), `board`, `launch`, `args`, `nodes`, `panic`,
      `profile`, `variant`, `conf` — the last two are **F3/F8**: a variant name
      and an explicit per-image overlay list, because `nano-ros-rt-eval` builds
      one app on one board twice (`prj-edf.conf` vs not) and
      `demo_bringup/ablation/*.toml` are whole alternate system configs.
- [ ] **W1.b** Add the `[image]` base table (RFC-0065 D5.1) merged under each
      `[image.<id>]`, so an eight-image workspace states its RMW and edition
      once. Precedent: PlatformIO's `[env]` / `[env:NAME]`.
- [ ] **W1.c** Add `[system] default_images`. **F7**: a workspace may declare
      several bringups (`nano-ros-rt-eval` has `demo_bringup` AND
      `load_bringup`), so resolve `default_images` per bringup and require
      `nros build <image>` to be unambiguous across all of them — a duplicate
      image id across two bringups is a hard error naming both files.
- [ ] **W1.d** `panic` is the **existing** RFC-0077 policy enum
      (`platform`|`halt`|`own`), forwarded to `nros::main!`. Do not invent
      values; `"semihosting"` is a crate, not a policy.
- [ ] **W1.e** Resolve `board` through `packages/boards/board-support.toml`
      (RFC-0065 D9): the user always writes a nano-ros board id, and the
      registry carries the framework's own board string for platforms that have
      one. `[image.*].board` today mixes seven nano-ros keys with one raw Zephyr
      string (`native_sim/native/64`) — the conflation phase-337 W2 removed from
      `PlatformId`, still live one layer up. Add `framework_board` to the
      registry rows that need it; `check-board-tiers` already validates the
      file's completeness.
- [ ] **W1.f** Deprecate `[deploy.*]` using phase-222's shipped pattern
      verbatim: both spellings parse, `[deploy.*]` warns once per invocation
      naming `[image.*]`, `NROS_SUPPRESS_DEPRECATION=1` opts out,
      `nros doctor` flags it in config files, deletion at the next minor
      version. **A version boundary, not a time period.**

**Acceptance.** `nros ws model-dims` over all nine in-tree workspaces returns
byte-identical output before and after the bringups are rewritten to
`[image.*]` — the schema change moves no resolved value.

---

## W2 — Discovery that survives a real workspace

Carries **F4, F9**. Delivers `nros build` for the cases needing **no**
generation (west, ESP-IDF, copy-out leaves), so the pipeline is proven before
any emitter exists.

- [ ] **W2.a** Stage 1 discovery = the `package.xml` walk **UNION the cargo
      members already visible**. **F4**: `nano-ros-rt-eval/src/island_clock/`
      is `Cargo.toml` + `src/` with no `package.xml`; a members list derived
      from the walk alone drops it and the build dies on an unresolved path
      dependency.
- [ ] **W2.b** Honour `.nros-ignore` / `COLCON_IGNORE` in the walk. **F9**:
      `nano-ros-rt-eval` vendors nano-ros as a submodule and `touch`es
      `.nros-ignore` so the walk does not descend into it. `nros-pkg-index`
      already knows both markers — assert it, do not re-implement it.
- [ ] **W2.c** Wire stages 1→2→3→5 with **no stage 4**: resolve the image,
      preflight, then `exec` the framework tool. Zephyr and ESP-IDF need no
      generated root (RFC-0065 D3), so this is a complete, shippable
      `nros build` for them.
- [ ] **W2.d** Stage 3 preflight (RFC-0065 D2): auto-fetch what the index can
      fetch **after prompting with the download size**; `--yes` skips the
      prompt; **a non-TTY behaves as verify-only** and never blocks.
      License-gated packages are never auto-fetched in any mode — they fail
      naming the package and the manual `nros setup` line. Reuse
      `nros setup --check`'s existing "verify, name what is missing, fetch
      nothing" path rather than a second implementation.
- [ ] **W2.e** Stage 5 is `execvp`, not a pipe, not `Command::output()`. The
      test asserts that a deliberately broken source produces **byte-identical
      stderr** to the native tool invoked directly. This is the whole RFC-0024
      §2.4 reconciliation; if it is a pipe, the amendment is void.

**Acceptance.** `nros build zephyr` in `examples/workspaces/rust` produces the
same artifact as today's `west build -b native_sim/native/64 …` line, and a
syntax error in `talker_pkg` yields identical stderr under both.

---

## W3 — The cargo root emitter

Carries **F10**.

- [ ] **W3.a** Emit `build/<coord>/Cargo.toml` with `[workspace] members` from
      W2.a's union, `exclude` for west/idf entries, and the workspace-level
      `[patch]` set `nros sync` already computes.
- [ ] **W3.b** **Emit the Rust entry** (RFC-0065 D4 — the heart of this
      phase). From `(launch, args, board)` produce `build/<coord>/<image>_entry/`
      carrying `Cargo.toml` (board crate, node rlibs, the `*_nros_selection`
      facade `nros sync` generates) and a `main.rs` whose body is the one-line
      `nros::main!(launch = …, args = …)` plus the board's shell —
      `#![no_std]`/`#![no_main]`, the `panic` policy from W7.a, and board
      boilerplate such as `esp_app_desc!()`. The existing emitters in
      `packages/cli/nros-cli-core/src/codegen/entry/emit_rust.rs` already
      produce this shape for `nros codegen entry`; this wave gives them
      `(image, board)` as input instead of a hand-written package.

- [ ] **W3.c** Output is **deterministic**: no timestamps, no absolute paths,
      stable ordering. Gate it the way issue 0320 gated model paths —
      `check-no-absolute-model-paths` is the pattern to copy. A production
      build's reproducibility depends on this and nothing else in the phase
      enforces it.
- [ ] **W3.d** `nros build a b c` builds several images in one invocation.
      **F10**: `nano-ros-rt-eval`'s `just build` is
      `cargo build -p native_entry -p peer_entry`.
- [ ] **W3.e** Bare `nros build` honours `default_images`; absent it, a
      multi-image workspace lists them and **fails**. `--all` opts in.
      `nano-ros-rt-eval`'s manifest documents why: a bare workspace build
      "would try [the cross-target member] for the host and fail".

**Acceptance.** `examples/workspaces/rust`'s tracked root `Cargo.toml` and the
generated one produce identical `cargo metadata` output modulo path prefixes.

---

## W4 — The cmake root emitter

Carries **F5**. This is the wave that must not fragment the corrosion cargo
tree — **one cmake configure per workspace** (RFC-0070's 80.6 GB measurement).

- [ ] **W4.a** Emit `build/<coord>/CMakeLists.txt` calling
      `nano_ros_workspace(BACKEND … PLATFORM … SYSTEM … ORDER_FROM_DEPENDS
      SUBDIRS …)`, with the board→toolchain map resolved **before**
      `project()` and the `NUTTX_DIR` promotion the hand-written roots do by
      hand.
- [ ] **W4.b** **Emit the C/C++ entry** — the `nano_ros_add_executable(BOARD …
      BRINGUP … LAUNCH … LANG … DEPLOY …)` call that the hand-written entry
      leaves carry today, with `BOARD`/`DEPLOY` taken from the resolved image
      rather than written as literals. This is what closes **issue 0798**: a
      generated entry cannot disagree with the board it was generated for.
      Embedded C/C++ entries already carry zero source, so for them this wave
      deletes the package and emits its one call.

- [ ] **W4.c** **User preamble hook.** **F5**: `autoware-safety-island`'s root
      calls `find_package(Eigen3 REQUIRED)`. An optional
      `<bringup>/cmake/preamble.cmake` is `include()`d before `project()` if
      present; absent, nothing changes.
- [ ] **W4.d** **A package that gates itself out is not an error.** **F5**:
      ASI adds `src/s32z2_board_glue` only when the NXP SDK is provisioned —
      "the pkg's own CMakeLists gates and reports". The emitter lists it; the
      package decides.
- [ ] **W4.e** Assert one cargo target dir per workspace configure, not per
      package. Test: configure a mixed workspace, then assert exactly one
      `cargo/build` tree under `$NROS_BUILD_ROOT`.

**Acceptance.** `examples/workspaces/mixed` builds from a generated root with
no tracked root `CMakeLists.txt` present, and `ninja -t query` shows the RMW
archive under `|` (the issue-0475 link-edge check).

---

## W5 — Zephyr overlays and multi-image output

Carries **F1, F2**. F1 is a **correction already made in the RFC** — implement
the corrected form.

- [ ] **W5.a** (RFC-0065 D10) Pass overlays as **`EXTRA_CONF_FILE`, never `CONF_FILE`**.
      Zephyr's `configuration_files.cmake` puts `boards/` and `socs/`
      auto-discovery inside `if(NOT DEFINED CONF_FILE)`, so setting `CONF_FILE`
      suppresses both. Our own zephyr entries suppress it today;
      `nano-ros-rt-eval`'s justfile carries the workaround note.
- [ ] **W5.b** Point `APPLICATION_CONFIG_DIR` at
      `src/<bringup>/boards/<board>/`. Verified reachable for sysbuild too:
      `share/sysbuild/cmake/modules/sysbuild_kconfig.cmake` resolves
      `sysbuild.conf` through it and FORCEs it into the cache so images inherit
      it.
- [ ] **W5.c** Map an image's `variant` onto Zephyr's own
      `prj_<buildtype>.conf` → `CONF_FILE_BUILD_TYPE` mechanism (**F2**),
      rather than inventing a parallel axis. `autoware-safety-island`'s
      `prj_actuation.conf` is already this shape.
- [ ] **W5.d** Detect sysbuild by the **presence of `sysbuild.conf`** in the
      board's config dir and pass `--sysbuild`. Zephyr's own comment:
      "sysbuild.conf is an optional file, because sysbuild is an opt-in
      feature." Invent no key.

**Acceptance.** A `native_sim` image built through `nros build` has a `.config`
identical to the one today's explicit `west build` line produces, including the
`boards/native_sim_native_64.conf` values that `CONF_FILE` currently drops.

---

## W6 — `dist/` and the manifest

- [ ] **W6.a** (RFC-0065 D8) `dist/<image>/` holds the artifact **set** plus `manifest.toml`
      naming which member is flashable and at what address. A host image is a
      set of one — the same shape, not a special case.
- [ ] **W6.b** **Completeness gate**: every file in `dist/<image>/` is named by
      that image's manifest; an unnamed artifact fails the build. ESP-IDF's
      `flasher_args.json` supplies the cautionary tale — a filed bug reads
      "missing entry for `bootloader` when built with secure boot v2", the
      manifest silently falling behind its artifacts.
- [ ] **W6.c** (RFC-0065 D14) `--offline`: stages 1–4 perform no network I/O, and stage 5 is
      invoked with the native tool's offline spelling (`cargo --frozen`).
      **The value is converting a silent fallback into a named failure** —
      `NanoRosCorrosion.cmake` falls through to
      `FetchContent_Declare(Corrosion GIT_REPOSITORY …)` when the SDK store has
      no matching prefix, and the file calls itself "offline-hostile".
- [ ] **W6.d** Amend RFC-0070 R1 in the same commit as W6.a lands
      (`install/` → `dist/`), so the two documents never disagree.

**Acceptance.** `nros build native --offline` in a tree with no Corrosion in
the SDK store fails naming the missing package, and `strace -f -e trace=connect`
records no outbound connection.

---

## W7 — The escapes

Carries **F6**.

- [ ] **W7.a** Forward `panic` and `profile` from `[image.<id>]` into the
      generated entry (RFC-0065 D5). These are the only escapes the D4 survey
      found; the custom spin loop already has RFC-0024 §11.8.
- [ ] **W7.b** `nros materialize <image>` writes `src/<image>_entry/`. Naming
      follows the **image**, which D6 made the named unit.
- [ ] **W7.c** **Shape stamp that WARNS, never errors.** **F6**:
      `autoware-safety-island` carries `freertos_main.cpp`, `board_init.c`,
      `cp15_arm.S` and four `.ld` fragments — it will hold a materialised entry
      **forever, by design**. An error would break a legitimate downstream
      permanently.
- [ ] **W7.d** A test that a materialised entry still builds. A decorative
      escape silently deletes capability.
- [ ] **W7.e** `nano_ros_support_library(<name> SRCS … INCLUDES … WHOLE_ARCHIVE)`
      (RFC-0065 D12). The keyword emits the flag **and** the `LINK_DEPENDS` —
      issue 0475 is the reason users must not hand-write it. Our CMake floor is
      3.22, so `$<LINK_LIBRARY:WHOLE_ARCHIVE,…>` (3.24+) is unavailable; owning
      the spelling means the floor can rise later with no user edit.
- [ ] **W7.f** Add `LINKER_FRAGMENTS` to the same function. **F6**: D12 covered
      libraries but not `.ld` fragments, and ASI has four. Zephyr's
      `zephyr_linker_sources()` is the seam.

---

## W8 — Hostile-workspace acceptance (GATES W9)

Our nine workspaces are uniform by construction. These two are not, and they
are where F4–F6 came from.

- [ ] **W8.a** Dry-run migrate `nano-ros-rt-eval`: two bringups, ablation
      variants, a cargo member with no `package.xml`, a vendored nano-ros
      submodule, a cross-target member, two images built together. Assert the
      generated root produces the same `cargo metadata` as its tracked one.
- [ ] **W8.b** Dry-run migrate `autoware-safety-island` (branch `nano-ros`):
      a hand-written `main`, four linker scripts, assembly, a self-gating
      NXP-licensed package, a root preamble, and concern-named conf files.
      Assert its FreeRTOS-POSIX lane still builds.
- [ ] **W8.c** Neither project is modified in this phase. A dry run that needs
      an upstream edit is a **W1–W7 defect**, not a migration task.

**This wave gates W9.** If it does not pass, the builder is not ready for the
in-tree migration regardless of how green our own examples are.

---

## W9 — Migrate the nine (RFC-0065 D13, stage 2)

- [ ] **W9.a** Migrate all nine workspace roots and their entry packages, one
      commit per workspace so a regression bisects to one.
- [ ] **W9.b** Update `examples/fixtures.toml` rows to invoke `nros build`.
      A row already describes `(image, board)`, so it becomes an invocation
      rather than a description of one.
- [ ] **W9.c** Re-run the tier the diff earns per CLAUDE.md — this touches
      `cmake/` and codegen, so **`just ci-matrix`** at minimum, with
      `just build-test-fixtures lane=tier2` first.

---

## W10 — Retire the old paths (RFC-0065 D13, stage 3)

- [ ] **W10.a** Delete the nine tracked root `CMakeLists.txt` and the eight
      root `[workspace]` manifests, plus the entry packages W9 made derivable.
- [ ] **W10.b** Delete `[deploy.*]` parsing at the version boundary W1.e
      declared.
- [ ] **W10.c** Add `check-no-tracked-workspace-roots` so the shape cannot
      return. Every gate in this repo exists because a class recurred; this one
      is cheap and the class is "someone re-adds a hand-written root".
- [ ] **W10.d** Book sweep: `examples/workspaces/*/README.md` still print the
      six-command ritual this phase deletes.

---

## Risks

- **W4 is the corrosion-fragmentation risk.** One cmake configure per workspace
  is what preserves the single cargo tree; a per-package configure would
  re-create the 151.7 GiB duplication RFC-0070 measured. W4.e asserts it
  directly rather than trusting the emitter.
- **W8 may reject the design, not just the code.** F4–F6 came from these two
  projects on a *reading*. Building against them may surface an F11 that needs
  an RFC amendment. That is the wave working as intended, and W8.c says so.
- **W2.e is load-bearing for the RFC-0024 amendment.** If stage 5 ever becomes
  a pipe — for a progress bar, for log capture — the reconciliation that makes
  `nros build` admissible is void. The test is the guard.
- **W9 is nine migrations with one shared cause.** If they land as one commit,
  a regression bisects to a 9-workspace diff. W9.a splits them for that reason
  alone.
- **The phase is large.** W1–W7 is a builder; W8–W10 is a migration. If it must
  be cut, W1–W3 plus W8.a is a coherent shippable subset (Rust workspaces only,
  proven against a hostile tree); W4–W7 then follow per language.
