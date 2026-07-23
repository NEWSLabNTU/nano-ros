# Pure C workspace

This template proves the pure-C Phase 223 workspace shape:

- C Node pkg: `src/c_talker_pkg`
- C Node pkg: `src/c_listener_pkg`
- Bringup pkg: `src/demo_bringup`
- C Entry pkg: `src/robot_entry`

Both Node pkgs are static libraries with no `main()`. The C Entry pkg
owns boot and links both Node pkg libraries through
`nano_ros_entry(LANG c MODEL "…/demo_bringup/config/system_model.yaml")` — the
resolved SystemModel emitted by `play_launch resolve` from the launch file.

```sh
cmake -S . -B build
cmake --build build
```
