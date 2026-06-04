# Pure C workspace

This template proves the pure-C Phase 223 workspace shape:

- C Node pkg: `src/c_talker_pkg`
- C Node pkg: `src/c_listener_pkg`
- Bringup pkg: `src/demo_bringup`
- C Entry pkg: `src/robot_entry`

Both Node pkgs are static libraries with no `main()`. The C Entry pkg
owns boot and links both Node pkg libraries through
`nano_ros_entry(LANG c LAUNCH "demo_bringup:system.launch.xml")`.

```sh
cmake -S . -B build
cmake --build build
```
