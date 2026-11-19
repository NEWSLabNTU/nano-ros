# Pure C workspace

This template proves the pure-C workspace shape:

- C Node pkg: `src/c_talker_pkg`
- C Node pkg: `src/c_listener_pkg`
- Bringup pkg: `src/demo_bringup` — its `system.toml` declares both
  components and the image, `[image.linux]`

There is no root `CMakeLists.txt` and no entry package:
`nros build` generates the cmake root and the image's C entry
(`linux_entry`) under `build/`. Both Node pkgs are static libraries with no
`main()`; the generated entry owns boot and links both from the SystemModel
resolved from the launch file.

```sh
source /path/to/nano-ros/activate.sh   # puts that checkout's nros on PATH
nros configure
nros build linux
./build/posix-zenoh-linux-demo_bringup/cmake/linux_entry
```
