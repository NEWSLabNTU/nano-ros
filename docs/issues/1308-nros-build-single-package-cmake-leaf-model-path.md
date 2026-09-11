---
id: 1308
title: "`nros build` in a single-package C/C++ leaf looks for the SystemModel under the package name, and sync writes it under the directory name"
status: open
type: bug
area: cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0065, rfc-0098, phase-445]
---

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
