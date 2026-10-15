C API: the headers moved from `nros/` into `rclc/` (RFC-0089 amendment
2026-10-15, phase-501). `<rclc/rclc.h>` is the umbrella, and the content
that was in `<nros/publisher.h>`, `<nros/log.h>`, `<nros/component.h>` and the
rest now lives in `<rclc/publisher.h>`, `<rclc/log.h>`, `<rclc/component.h>`.
Entry, configuration and generated-code support moved to `<rclc/detail/*.h>`,
which is internal. No forwarding headers remain, so an `#include <nros/…h>` of
an nros-c header no longer compiles: include `<rclc/rclc.h>` (or the
upstream-path header you need). Port-seam headers owned by other packages
(`<nros/platform.h>`, `<nros/rmw_vtable.h>`, `<nros/transport.h>`) are
unchanged. `<rclc/types.h>` now brings the types alone; every other
upstream-path header still brings the whole API, as before. The codegen
version is now 16, with minimum 16, because generated C message headers
include the new paths: re-run `nros sync` (or rebuild, which regenerates) for
any generated tree from an earlier release.
