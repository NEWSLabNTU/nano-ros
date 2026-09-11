# C and C++ mixed workspace

This template is the Phase 223 reference shape:

- C Node pkg: `src/c_talker_pkg`
- C++ Node pkg: `src/cpp_listener_pkg`
- Bringup pkg: `src/demo_bringup` — its `system.toml` declares the image,
  `[image.native]`

There is no root `CMakeLists.txt` and no entry package (RFC-0098 D9, RFC-0065
D4): a workspace is a directory of packages, and `nros build` generates the
cmake root and the image's entry (`native_entry`) under `build/`. The C Node
pkg is a static library with no `main()`; the generated entry owns boot and
links both Node pkg libraries from the SystemModel resolved from the launch
file.

```sh
export NROS_REPO_DIR=/path/to/nano-ros
nros sync
nros build native
./build/posix-native/cmake/native_entry
```
