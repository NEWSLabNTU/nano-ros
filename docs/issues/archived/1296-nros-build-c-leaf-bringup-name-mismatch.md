---
id: 1296
title: "`nros build` in a single-package C/C++ leaf fails — the synthesised bringup is named after the DIRECTORY, the cmake driver asks for `[system] name`"
status: resolved
type: bug
area: tooling, cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0098, phase-445]
resolved_in: "phase-445 (plan_single_package_cmake)"
---

## Resolved (2026-10-11)

The name mismatch was the first of two layers. Naming the bringup the same way
on both sides got `nros plan` to the model, and then it refused for "missing
source metadata". The workspace road had generated a root for a workspace of
one, with a synthesised typed `<id>_entry` whose BRINGUP named
`src/<[system] name>`, a directory no leaf has. A single-package C/C++ leaf is
not a workspace: it declares its own executable in its own `CMakeLists.txt`,
and RFC-0098 leaves that CMake side as it is.

So `nros build` now has a cmake twin of the cargo leaf road,
`plan_single_package_cmake` in `packages/cli/nros-cli-core/src/cmd/build.rs`.
For a leaf with `CMakeLists.txt` and `system.toml`, no `Cargo.toml`, and a
board whose road is cmake, it configures the leaf's OWN `CMakeLists.txt` into
`build/<image>/cmake`. It passes `-DNROS_IMAGE=<id>` (which
`nano_ros_read_leaf_system` honours), the board descriptor's toolchain file,
the board heap and `nano_ros_ROOT`, and then runs `cmake --build`. It is the
same pair the book documents, with the inputs a bare configure cannot know. A
west or idf leaf returns `None` and keeps its road.

Measured on the branch:
- `examples/native/c/talker` and `examples/native/cpp/talker`: `nros sync`
  then `nros build`, both rc=0.
- `examples/mps2-an385-freertos/c/talker`: `nros build`, rc=0 (the
  `arm-freertos-armcm3.cmake` toolchain was passed).
- The same native C command on `main` fails with
  `no SystemModel for native_c_talker/launch/system.launch.xml`.

Tests: `single_package_tests::a_cmake_leaf_configures_its_own_cmakelists_for_its_image`
and `…a_cmake_leaf_refuses_an_image_it_does_not_declare`. Both fail with the
road disabled.

# What happens

In a single-package C leaf converted by phase-445 W3b, `nros sync` succeeds and
`nros build` then fails at cmake configure:

```console
$ cd examples/native/c/talker
$ nros sync
sync: resolved system.launch.xml → build/nros/models/talker/system_model.yaml
sync: done.
$ nros build
nros build: native_c_talker:native -> board native (platform posix), driver cmake
CMake Error at cmake/nano_ros_workspace_metadata.cmake:68 (message):
  nano_ros_workspace_metadata: `nros plan` failed (rc=1)
  stderr: Error: no SystemModel for `native_c_talker/launch/system.launch.xml`.
    It is a BUILD ARTIFACT (phase-330 W4), so generate it rather than
    committing one:
      nros sync            # writes <ws>/build/nros/models/<bringup>/
```

The remedy the message names is the command that was just run, so the error is
also a dead end for the reader.

# Why

The leaf's synthetic bringup identity is derived TWICE, from two different
facts, and they disagree:

- `ws.rs`'s `synthesise_leaf_launch` names it after the leaf's **directory**:
  `<leaf>/build/nros/launch/<leaf-dir>/system.launch.xml` — here `talker`.
- `cmake/nano_ros_workspace_metadata.cmake` asks `nros plan` for
  `${_NRW_SYSTEM}`, the **`[system] name`** — here `native_c_talker` — and
  builds `${_bringup_dir}/launch/system.launch.xml` from it.

For `examples/native/c/talker` those are `talker` and `native_c_talker`. The
two agree only where a leaf's directory name happens to equal its `[system]
name`, which is true of no converted leaf I checked.

The same disagreement exists for a Rust leaf (`examples/native/rust/talker`
resolves to `build/nros/models/talker/` while `[system] name` is
`native_talker`) and is simply never asked: `nros build` takes the cargo driver
there, which reads the model through `model_location` rather than through
`nros plan`.

# Consequence

`nros build` is usable in a single-package **Rust** leaf and not in a
single-package **C or C++** one. The C/C++ leaf path that does work is the
leaf's own CMake, which does not go through `nano_ros_workspace` at all:

```console
$ cd examples/native/c/talker
$ cmake -B build && cmake --build build      # measured green, with and without a prior `nros sync`
```

So this blocks phase-445's acceptance sentence ("a fresh clone, then
`nros build <image>`, builds one image per board family") for the C and C++
families only, and it is why `book/src/getting-started/first-node-c.md` and
`first-node-cpp.md` still document the leaf `cmake` pair rather than
`nros build` (phase-445 W7).

# Fix

One derivation, as usual. Either:

- the synthesiser names the bringup dir from the `[system] name` it already
  read (`decl`), so both sides answer the same question; or
- `nano_ros_workspace_metadata` receives the bringup directory rather than
  recomputing it from `_NRW_SYSTEM`.

The first is preferable — `[system] name` is the authored fact, the directory
name is an accident of where the user put the package — but it moves a path
that other readers may key on, so the change wants a sweep over every consumer
of `build/nros/launch/<x>/` and `build/nros/models/<x>/`, plus an acceptance
BUILD of a C leaf, a C++ leaf and a Rust leaf through `nros build`.

# Measured

2026-09-11, worktree at `efd1c2c57` + phase-445 W4b/W5 cherry-picks:

- `examples/native/c/talker`: `nros sync` OK, `nros build` fails as above;
  `cmake -B build && cmake --build build` produces `build/c_talker` (10.6 MB).
- A copy of the same leaf outside the checkout, with no `nros sync` at all,
  configures and builds identically — the C leaf's message bindings are a
  CMake-time output, so sync is not a precondition there.
- `examples/native/rust/talker`: `nros sync` + `nros build` both OK,
  `build/native/target/debug/talker`.
