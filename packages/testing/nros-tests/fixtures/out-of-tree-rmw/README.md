# out-of-tree-rmw — an RMW backend nano-ros does not know about

RFC-0071 D16 (phase-545 W3), the acceptance test for "a backend added
out-of-tree with zero core edits", on the `nros build` road.

- `src/acme_rmw/` is the provider: a `package.xml` announcing
  `<nano_ros_provides kind="rmw" name="acme"/>`, an `nros-rmw.toml`, and the
  cargo crate `acme-rmw`. It implements the RMW seam by delegating to zenoh,
  and prints a marker on the first sample published through it.
- `src/talker_pkg/` publishes `std_msgs/String` on `/chatter`.
- `src/demo_bringup/` declares two images, both `rmw = "acme"`: `linux`
  (`board = "linux"`, the cargo road) and `zephyr` (`board = "zephyr"`, native_sim,
  the west road, issue 2372). On Zephyr, acme states the module backend it is
  built over, `[rmw.zephyr] module_backend = "zenoh"` (RFC-0071 D18).

Nothing under `packages/` outside this directory names `acme`.

- `nros-cli-core/tests/out_of_tree_rmw_build.rs` drives `nros build`'s
  configure and entry generation over a copy and asserts the generated entry
  path-depends on `acme-rmw` and registers it.
  A second test does the same for the `zephyr` west application, and
  checks the module hook's Kconfig fragment selects `CONFIG_NROS_RMW_ZENOH`.
- `nros-tests/tests/out_of_tree_rmw_e2e.rs` runs the image the
  `workspace-out-of-tree-rmw` row in `examples/fixtures.toml` builds, against a
  router, and sees the talker publish through backend `acme`.
- `nros-tests/tests/out_of_tree_rmw_zephyr_e2e.rs` does the same for the
  `zephyr` image, which the Zephyr lane builds from row
  `workspace-zephyr-out-of-tree-rmw`.
