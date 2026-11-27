# probe_seeds_ws — the metadata probe constructs a node with its launch values

Issue 2640 (phase-566). Two components, one C and one C++, each declare
`ctrl_period_ms` WITHOUT a default and take it from the launch file, and each
declares 40 more parameters, more than a default parameter store holds (32):

- `seeds_c_pkg::Controller` (C, `configure` shape) declares it with
  `rclc_add_parameter` and refuses to configure when it reads 0;
- `seeds_cpp_pkg::Controller` (C++, `rclcpp` shape) declares it with
  `declare_parameter<int64_t>(name)` and creates its timer only when the period
  is positive.

Issue 2901: the launch file has a second C++ instance, `seeds_cpp_alt`. It
sets the same `ctrl_period_ms` and a different `gain_03`, a parameter that has
a default in code. The probe seeds every key the two instances agree on and
leaves only `gain_03` on its default, so the C++ sidecar records the timer and
a `probe.partly_seeded` object naming `gain_03`.

The host metadata probe seeds each node with the values the image is baked
with, so each sidecar records the timer, all 41 parameters and a
`probe.seeds` object. `tests/probe_seeds_workspace.rs` reads them; the
`workspace-probe-seeds` row in `examples/fixtures.toml` builds them.
