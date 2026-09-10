# C Workspace

This workspace demonstrates C Node packages with a C native Entry package.

```text
c/
├── .colcon_workspace
└── src/
    ├── talker_pkg/    # Node pkg: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/  # Node pkg: subscribes std_msgs/Int32 on /chatter
    ├── demo_bringup/    # Bringup pkg: package.xml + system.toml + launch/
    └── zephyr_entry/    # the ONE entry pkg — west needs a real app dir
```

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/c
nros setup native
nros sync            # once per workspace: generated message bindings
nros build native
```

`nros build` walks the packages, resolves the image, checks the
toolchain, generates what the build system needs, and hands off to
cargo/cmake/west — so compiler errors are the compiler's, unchanged
(RFC-0065). `nros build` with no image lists what this workspace
declares. The old `codegen-system` / `check` steps still work; they are
just no longer something you have to type — but `nros sync` still is.
A workspace that has never been synced has no generated message crates,
so `nros build` refuses at preflight and names `nros sync` as the remedy.
