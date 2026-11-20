# zephyr-module-rmw -- an RMW package that is its own Zephyr module

RFC-0140 D6 (phase-581 W8), the acceptance fixture for the open RMW set on
Zephyr: a backend with Zephyr code of its own joins an image with no edit
outside its package.

- `src/stub_rmw/` is the provider: a `package.xml` announcing
  `<nano_ros_provides kind="rmw" name="stub"/>`, an `nros-rmw.toml` stating
  `[rmw.zephyr] module = "zephyr"`, the cargo crate `stub-rmw` (a stub
  transport: the table names the backend and nothing else), and `zephyr/`, the
  package's own Zephyr module (`module.yml`, a `Kconfig` and a `CMakeLists.txt`
  that compiles `stub_transport.c` and prints
  `stub_rmw: the stub RMW package's own Zephyr module CMake runs`).
- `src/talker_pkg/` publishes `std_msgs/String` on `/chatter`.
- `src/demo_bringup/` declares one image, `zephyr` (`board = "zephyr"`,
  native_sim), with `rmw = "stub"`.

On configure, the nano-ros module hook adds `src/stub_rmw` to the build's
modules and writes `NROS_RMW_STUB` into `<build>/nros/Kconfig.packages` as a
member of `choice NROS_RMW_BACKEND`; the image's fragment selects it. Nothing
under `packages/` outside this directory names `stub`.

`nros-cli-core/tests/out_of_tree_rmw_build.rs` drives the configure's Kconfig
render over a copy and asserts the member, the module row and the choice;
`docs/roadmap/data/phase-581-w8-measure.md` records the Zephyr 3.7
`native_sim/native/64` configure that ran the module's CMake.
