# Pure C workspace

This template proves the pure-C Phase 223 workspace shape:

- C Node pkg: `src/c_talker_pkg`
- C Node pkg: `src/c_listener_pkg`
- Bringup pkg: `src/demo_bringup` — its `system.toml` declares both
  components and the image, `[image.native]`

There is no root `CMakeLists.txt` and no entry package (RFC-0098 D9, RFC-0065
D4): `nros build` generates the cmake root and the image's C entry
(`native_entry`) under `build/`. Both Node pkgs are static libraries with no
`main()`; the generated entry owns boot and links both from the SystemModel
resolved from the launch file.

```sh
export NROS_REPO_DIR=/path/to/nano-ros
nros sync
nros build native
./build/posix-native/cmake/native_entry
```
