# image_config_ws — per-image configuration on the workspace cmake road

phase-481 W4 (RFC-0098 D12). Two native images on ONE coordinate:

- `plain` states nothing, so it keeps `build/posix-zenoh-native/`;
- `warn` states `[image.warn] env`, so it gets its own configure
  (`build/posix-zenoh-native-cfg<hash>/`) and its own runtime build.

`probe_pkg`'s component prints what its image compiled in;
`tests/image_config_workspace.rs` asserts each binary saw its own image's
configuration. Built by the `workspace-image-config-*` rows in
`examples/fixtures.toml`.
