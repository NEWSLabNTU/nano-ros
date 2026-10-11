---
id: 1308
title: "`nros build` in a single-package C/C++ leaf looks for the SystemModel under the package name, and sync writes it under the directory name"
status: resolved
type: bug
area: cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0065, rfc-0098, phase-445]
resolved_in: "phase-445 (plan_single_package_cmake)"
---

Same fix as issue 1296, which this is the other side of.

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

In a single-package C or C++ example — `examples/native/c/talker`, converted to
`system.toml` by phase-445 W3b — `nros sync` succeeds and `nros build` then
fails at CONFIGURE:

```
$ nros sync examples/native/c/talker
sync: resolved system.launch.xml → build/nros/models/talker/system_model.yaml
sync: done.

$ cd examples/native/c/talker && nros build
CMake Error at cmake/nano_ros_workspace_metadata.cmake:68 (message):
  nano_ros_workspace_metadata: `nros plan` failed (rc=1)
  stderr: Error: no SystemModel for `native_c_talker/launch/system.launch.xml`.
  (searched, in order: $NROS_MODEL_DIR, $OUT_DIR/nros,
   <ws>/build/nros/models, <bringup>/build/nros/models.)
```

The two halves disagree about ONE path component:

| | key used |
|---|---|
| `nros sync` writes | `build/nros/models/**talker**/system_model.yaml` — the leaf DIRECTORY name |
| `nros plan` looks up | `build/nros/models/**native_c_talker**/…` — the `package.xml` / `[system] name` |

`examples/native/c/talker/system.toml` carries `[system] name =
"native_c_talker"` and `<name>native_c_talker</name>`, while the directory is
`talker`. A leaf whose directory name happens to equal its package name would
not show this.

Reproduced on `examples/native/c/talker` and `examples/native/cpp/talker`, both
from a clean `build/`.

# What this is NOT

Not phase-445 W6. A C/C++ leaf has never had a `.cargo/` directory, so nothing
W6 deleted is on this path, and W6's diff touches no model resolution — its
`cmake/` and `orchestration/` changes are comment-only plus one new
`ImageBlock` field. The regression window is phase-445 W5, which rewrote 294
lines of `cmd/build.rs` and added `nano_ros_workspace_metadata` to
`cmake/NanoRosWorkspace.cmake`; that branch has no PR yet, so this has not
reached `main`.

# Why it stayed invisible

`nros build` is not how the lane builds these leaves. `examples/fixtures.toml`
carries three rows for `examples/native/c/talker` (zenoh, xrce, cyclonedds) and
the fixture builder drives cmake directly, so the native lane is green while
the command RFC-0098 D2 names as "the one command that needs no flags" is not.

# The fix

One key, not two. `model_location` is the tree's single answer to "where does a
SystemModel live" (phase-330 W4.a); whichever side is wrong should ask it
rather than derive a name. Decide from the writer: sync resolved the synthesised
`system.launch.xml` under the leaf directory, so either sync should key on the
package name or the lookup should key on the bringup DIRECTORY — and the choice
has to be the same one a workspace bringup already makes, or a leaf and a
workspace will keep disagreeing.

Acceptance is a BUILD: `nros sync && nros build` in
`examples/native/{c,cpp}/talker` from a clean `build/`.
