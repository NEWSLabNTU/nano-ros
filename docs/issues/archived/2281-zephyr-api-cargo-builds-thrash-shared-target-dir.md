---
id: 2281
title: "A Zephyr C/C++ image relinked on every `nros build` / `nros run`: two always-dirty edges, the API crates' shared cargo dir and a forced west re-configure"
status: resolved
type: bug
severity: medium
area: [zephyr, build, cmake]
related: [0616, 1100, 1156, 1824]
found: 2026-11-05
---

## What

`nros run` builds the image first (`run_exec::build_image`, an `nros build`),
and on a Zephyr C or C++ image that build relinked `zephyr.elf` every time,
with no source change. Found by phase-524 W4b on the Nucleo-F767ZI Renode
twin; measured here on `examples/zephyr/c/talker` (native_sim, the same
Zephyr module road):

```
$ nros build zephyr_zenoh --zephyr-workspace ~/.nros-box/workspaces/zephyr/3.7   # twice
...
[3/45] Building nros-c via Cargo
   Compiling nros-c v0.6.0 (packages/api/nros-c)
   Compiling nros-c-image v0.0.0 (.../nros/image/nros-c)
[4/11] Building nros-cpp via Cargo
   Compiling nros-c v0.6.0 (packages/api/nros-c)
   Compiling nros-cpp v0.6.0 (packages/api/nros-cpp)
   Compiling nros-cpp-image v0.0.0 (.../nros/image/nros-cpp)
[6/9] Linking C static library modules/nros/libnros.a
[7/9] Linking CXX executable zephyr/zephyr.elf
[8/9] Building native simulator runner, and linking final executable
```

The same happens with plain `ninja -C <west build dir>`, twice in a row, so it
is not the west reconfigure: two cargo invocations undo each other.

## Why

Two causes, each enough on its own; measured one after the other.

### 1. nros-c and nros-cpp rebuilt each other's units

The Zephyr module builds two generated image crates, `nros-c-image` and
`nros-cpp-image` (one cargo workspace, `<build>/nros/image`), as two custom
targets, each `cargo build -p <one of them>`, into ONE `--target-dir`
(`<build>/nros-rust`). Cargo unifies features per invocation, and the two
invocations unify differently: measured with `--unit-graph`, the nros-cpp
build enables `nros/log`, `nros/macros`, `nros-rmw/log` and
`nros-platform/alloc` (and different build-dependency features) that the
nros-c build does not. Both builds then compile `nros-c` into the same
fingerprint directory (`.fingerprint/nros-c-5e98f2023055849a`) against
different dependency units, so each build finds the other's rlib and rebuilds
it (`dirty: UnitDependencyInfoChanged`, then `StaleDependency` for
`nros-c-image`). The rebuilt archives are new files, so the image relinks.

The per-build feature strings in `zephyr/CMakeLists.txt` (issues 1100, 1156)
already align the features the two builds pass to nros-c itself. They cannot
align the transitive set, which is cargo's to unify, and the comment there
records the third drift of that hand copy.

### 2. `nros build` made west re-configure on every run

With cause 1 fixed, plain `ninja` is a no-op, but a second `nros build` still
printed `-- west build: generating a build system` and then

```
[2/11] Generating misc/generated/syscalls.json, misc/generated/struct_tags.json
[3/11] Generating linker_zephyr_pre0.cmd
[5/9] Building C object modules/nros/CMakeFiles/nros.dir/__/__/nros_app_register_backends.c.obj
[6/9] Linking C static library modules/nros/libnros.a
[7/9] Linking CXX executable zephyr/zephyr.elf
```

West re-runs CMake whenever it is handed CMake arguments
(`zephyr/scripts/west_commands/build.py`: `if (self.args.cmake or
self.args.cmake_opts or ...): self.run_cmake = True`), and `nros build` always
hands it some after `--` (the overlays, `ZEPHYR_EXTRA_MODULES`, the board's
facts). Zephyr's configure always touches
`misc/generated/syscalls_subdirs.trigger` (`scripts/build/subfolder_list.py`:
"Always touch trigger file to ensure json files are updated"), which
regenerates the syscall JSON and everything after it. `ninja -d explain` after
a bare `cmake <build dir>`: `restat of output zephyr/misc/generated/syscalls.json
older than most recent input zephyr/misc/generated/syscalls_subdirs.trigger`.

### Roads

The host CMake road is not affected (measured: a second `nros build` of
`examples/linux/c/talker` relinks nothing). Both causes belong to the Zephyr
west road (`zephyr/cmake/nros_cargo_build.cmake` and the `Driver::West`
handoff), so every west-built image shares them: native_sim, the Zephyr QEMU
boards and the Nucleo Renode twin. The CMake road of the other RTOSes builds
the API crates through `cmake/` (Corrosion), not this module.

## Resolution

One fix per cause.

1. **One cargo target dir per API crate.** `_nros_root_cargo_dir`
   (`zephyr/cmake/nros_cargo_build.cmake`) takes the package. nros-c keeps
   `<build>/nros-rust`, which is also the per-image header root
   (`NROS_GENERATED_HEADER_DIR`), so no header moves. nros-cpp builds into
   `<build>/nros-rust-cpp`. With `NROS_SHARED_CARGO_ROOT` set, `package=` is a
   key field: the two feature strings differed by one duplicated `alloc` in the
   measured image, so equal strings are a real possibility. The cost is the
   units both builds used to resolve identically (third-party crates and build
   dependencies), now compiled once per directory on a cold build. The nros
   crates their unification splits were already compiled twice.
   `scripts/build/gc-zephyr-builds.py` sweeps `nros-rust-*` too.
2. **Configure once per distinct west command.**
   `builder::zephyr::configure_once` records a digest of the whole west
   handoff (program, arguments, added environment) as
   `-DNROS_WEST_CONFIGURE_SHA256=<digest>`. A later plan whose build dir is a
   finished configure of the same digest (a `build.ninja`, and the digest in
   `CMakeCache.txt`) hands off `cmake --build <dir>`. That is what `west build`
   runs once it has decided not to configure. A changed board, overlay, module,
   environment or resolve record changes the digest and goes to west. So does
   any west option the caller passes (`-p`, `-t`, `-d`). The configure's own
   inputs stay on `CMAKE_CONFIGURE_DEPENDS`, as for a plain repeated
   `west build`.

## Verified

- `examples/zephyr/c/talker` (native_sim), `nros build` three times after the
  fix: the first configures (the new digest argument) and relinks, the second
  and third run `cmake --build`. Cargo is `Finished` with nothing compiled and
  `zephyr.elf` / `zephyr.exe` keep their mtime.
- The reported road, the Nucleo-F767ZI Renode twin (`nros new nucleo_ws
  --board nucleo-f767zi-zephyr --template talker --lang c` plus the serial
  locator line, `NROS_ZEPHYR_WORKSPACE` set): `nros build`, `nros build`,
  `nros run --expect 'Publishing:'`, `nros run --expect 'Publishing:'`. Both
  runs matched `Publishing: 'Hello World: 1'` in about 8 s, and `zephyr.elf`
  kept one mtime across all four.
- Plain `ninja -C <build dir>` twice: cargo `Finished` with nothing compiled
  for both crates, no relink.
- Gate `zephyr-api-cargo-target-dirs` (fast line,
  `packages/testing/nros-tests/tests/zephyr_api_cargo_target_dirs.sh`): the
  two packages get different directories in both modes, and nros-c keeps
  `nros-rust`. Mutation (the module reverted): both assertions fail.
- `configure_once_tests::a_repeated_west_command_builds_without_configuring_again`
  and `build_verb_pipeline::a_repeated_west_build_does_not_configure_again`.
  Mutation (`configure_once` never skips): both fail.
