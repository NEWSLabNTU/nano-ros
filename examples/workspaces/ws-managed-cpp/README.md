# `ws-managed-cpp` — a C++ lifecycle node that manages ITSELF

Phase 270 (issue 0103). This workspace exists because the lifecycle capability
has two shapes, and only one of them fits in the `features` workspace.

The `features` bringup asks the ENTRY CODEGEN to drive the machine: it emits
`nros_cpp_lifecycle_autostart(...)` so the node boots straight to `active`. That
is the declarative shape, and it is what `[lifecycle] autostart` in a bringup's
`system.toml` selects.

Here the node drives ITSELF. `ManagedTalker` uses the `nros::LifecycleNode`
wrapper and calls `register_services()` + `autostart(Active)` from its own
install hook, so its `system.toml` carries **no `[lifecycle]` block** — the entry
just boots and spins. Wiring lifecycle on both sides would register the REP-2002
services twice.

## Layout

```
src/
  demo_bringup/          system.toml + launch/system.launch.xml
  cpp_lifecycle_talker_pkg/   the self-managing node
  native_managed_entry/       entry: boots + spins, no lifecycle wiring
```

## Building

```sh
cmake -S . -B build -DNANO_ROS_PLATFORM=posix
cmake --build build
```

## Why it stayed out of `features`

Phase-331 W2 folded the per-feature workspaces into `features`, but that
workspace's bringup selects the entry-driven lifecycle shape for every one of
its lifecycle entries. Keeping the self-managed variant separate is what makes
the distinction testable: the two shapes differ precisely in whether the ENTRY
emits the autostart call, and a single bringup cannot demonstrate both.
