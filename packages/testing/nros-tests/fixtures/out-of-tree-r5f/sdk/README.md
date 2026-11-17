# fixture-r5f-sdk

The vendor SDK stand-in for the out-of-tree board fixture `fixture-r5f-freertos`
(phase-575 W2, phase-577 W8). It plays a released BSP root: a `VERSION` file,
`include/` and `src/`. A real board package wraps its vendor's BSP, which
nano-ros never fetches; this one is written from public ZynqMP register maps so
that CI needs no vendor download.

The board package finds it through its own variable, `FIXTURE_R5F_SDK_ROOT`,
set in the environment or passed to the configure. There is no row for it in
`nros-sdk-index.toml` (RFC-0135 D2).
