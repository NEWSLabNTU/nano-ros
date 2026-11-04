# C Workspace

This workspace demonstrates C Node packages with a C native Entry package.
Its talker, listener, AddTwoInts and Fibonacci packages are the ROS 2
tutorial programs (`Publishing: 'Hello World: N'`, `I heard: [...]`,
`Result of add_two_ints: 5`, `Result received: [...]`), the same programs the
standalone examples run (phase-535, RFC-0122).

```text
c/
├── .colcon_workspace
└── src/
    ├── talker_pkg/    # Node pkg: publishes std_msgs/String "Hello World: N" on /chatter
    ├── listener_pkg/  # Node pkg: subscribes std_msgs/String on /chatter
    ├── demo_bringup/    # Bringup pkg: package.xml + system.toml + launch/
    └── zephyr_entry/    # the ONE entry pkg — west needs a real app dir
```

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/c
nros setup linux
nros configure            # once per workspace: generated message bindings
nros build linux
```

`nros build` walks the packages, resolves the image, checks the
toolchain, generates what the build system needs, and hands off to
cargo/cmake/west — so compiler errors are the compiler's, unchanged
(RFC-0065). `nros build` with no image lists what this workspace
declares. The old `codegen-system` / `check` steps still work; they are
just no longer something you have to type — but `nros configure` still is.
A workspace that has never been synced has no generated message crates,
so `nros build` refuses at preflight and names `nros configure` as the remedy.
