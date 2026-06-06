# C++ Workspace

This workspace demonstrates C++ Node packages with a C++ native Entry package.

```text
cpp/
├── CMakeLists.txt
└── src/
    ├── talker_pkg/      # Node pkg: publishes std_msgs/Int32 on /chatter
    ├── listener_pkg/    # Node pkg: subscribes std_msgs/Int32 on /chatter
    ├── demo_bringup/    # Bringup pkg: package.xml + system.toml + launch/
    └── native_entry/    # Entry pkg: native main()
```

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/cpp
nros setup native
nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build
cmake --build build
```
