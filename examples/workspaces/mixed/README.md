# Mixed C / C++ Workspace

This workspace demonstrates mixed-language Node packages with a C++ native Entry
package.

```text
mixed/
├── CMakeLists.txt
└── src/
    ├── c_talker_pkg/      # C Node pkg: publishes std_msgs/Int32 on /chatter
    ├── cpp_listener_pkg/  # C++ Node pkg: subscribes std_msgs/Int32 on /chatter
    ├── demo_bringup/      # Bringup pkg: package.xml + system.toml + launch/
    └── native_entry/      # Entry pkg: native main()
```

From the repository root:

```bash
source ./activate.sh
cd examples/workspaces/mixed
nros setup native
nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build
cmake --build build
```
