---
id: 1722
title: "XRCE's service request buffer is not sized from the declared parameters, so a large `set_parameters` can still be dropped there"
status: open
type: bug
area: rmw, xrce, params, sizing
severity: medium
found: 2026-10-06
related: [issue-1352, phase-461, phase-480]
---

## What this is

Issue 1352 is the parameter family's request outgrowing the transport buffer
it lands in. Its fix (phase-480 W5) covers zenoh: the builtin inbox slot is
derived from the contract's declared parameters, refuses the build when it
cannot price them, and never falls below the old floor. Cyclone takes requests
into the executor's own parameter buffer, which phase-446 F3 already derives.

XRCE has neither. Every service server's request lands in
`XRCE_SERVICE_REQUEST_BUFFER_SIZE` (`nros-rmw-xrce/src/internal.h`). That
defaults to `XRCE_BUFFER_SIZE`, which is 1024. `XrceDemand` in
`nros-rmw-xrce-cffi/build.rs` derives the subscriber family and the stream
history from the sizing descriptor, but not the service request family. It
never reads `[params] service_shape`. So the 25-parameter `set_parameters`
that 1352 measured, about 2,411 B, does not fit an XRCE image's default
request buffer either, whatever the contract declares.

## Not measured

This is reasoned from the source. No XRCE image was run with a 25-name
`set_parameters`. Measure first: build an XRCE image whose contract declares
25 parameters, call `set_parameters` with all of them through the Agent, and
see whether a reply comes back.

## What a fix needs

- Derive `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` from the larger of the user
  services' request bound and the parameter family's request bound. Use the
  same token and the same rule zenoh uses
  (`nros-rmw-zenoh/build_param_slot.rs`). A third copy of the arithmetic is
  issue 1025's defect, so share it rather than restate it.
- Refuse when the shape declares a string or array parameter and nothing
  states the size, as zenoh does.
- The Agent's own MTU and stream fragmentation are a second limit. The fix
  should say which one binds.
