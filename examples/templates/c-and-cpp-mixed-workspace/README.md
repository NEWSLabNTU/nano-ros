# C and C++ mixed workspace

This template is the Phase 223 reference shape:

- C Node pkg: `src/c_talker_pkg`
- C++ Node pkg: `src/cpp_listener_pkg`
- Bringup pkg: `src/demo_bringup`
- C++ Entry pkg: `src/robot_entry`

The C Node pkg is a static library with no `main()`. The C++ Entry pkg
owns boot and links both Node pkg libraries through
`nano_ros_entry(LAUNCH "demo_bringup:system.launch.xml")`.

```sh
cmake -S . -B build
cmake --build build
```
