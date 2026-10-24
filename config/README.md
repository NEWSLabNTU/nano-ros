
## zenoh build data

A platform file here holds the platform's half of a vendored C build:
`arch_profiles`, `compile`, `required_env` and its `[arch.*]` profiles. The
zenoh-pico half is `[port.<platform>]` in
`packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml` (phase-349 W2b). Both
schemas, and how a build joins them, are in the book chapter
[`internals/zpico-build.md`](../book/src/internals/zpico-build.md).
