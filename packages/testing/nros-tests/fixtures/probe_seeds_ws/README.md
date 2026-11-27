# probe_seeds_ws — the metadata probe constructs a node with its launch values

Issue 2640 (phase-566). Two components, one C and one C++, each declare
`ctrl_period_ms` WITHOUT a default and take it from the launch file, and each
declares 40 more parameters, more than a default parameter store holds (32):

- `seeds_c_pkg::Controller` (C, `configure` shape) declares it with
  `rclc_add_parameter` and refuses to configure when it reads 0;
- `seeds_cpp_pkg::Controller` (C++, `rclcpp` shape) declares it with
  `declare_parameter<int64_t>(name)` and creates its timer only when the period
  is positive.

The host metadata probe seeds each node with the values the image is baked
with, so each sidecar records the timer and all 41 parameters. The C
component's sidecar carries a `probe.seeds` object.

The C++ component is launched twice (issue 2901), as `seeds_cpp` and
`seeds_cpp_b`: the two agree on `ctrl_period_ms` and differ on `gain_03`,
which has a declaration default. The probe seeds the period and leaves
`gain_03` on its default, so its sidecar still records the timer, and its
`probe.partial_seeds` object names `gain_03` as defaulted. `tests/probe_seeds_workspace.rs` reads them; the
`workspace-probe-seeds` row in `examples/fixtures.toml` builds them.
