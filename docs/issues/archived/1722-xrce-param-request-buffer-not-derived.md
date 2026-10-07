---
id: 1722
title: "XRCE's service request buffer is not sized from the declared parameters, so a large `set_parameters` can still be dropped there"
status: resolved
type: bug
area: rmw, xrce, params, sizing
severity: medium
found: 2026-10-06
related: [issue-1352, issue-1743, phase-461, phase-480]
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

## Not measured (as filed, 2026-10-06)

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

## Resolution

Resolved 2026-10-07 (phase-480 W7). The class is the parameter family's
request against every buffer it crosses on XRCE.

**Measured on main first.** The image was the `features` workspace's
param_talker built for XRCE (new image `native_rust_params_xrce`, row
`workspace-features-rust-params-xrce`). Its contract declares 25 integers with
35-byte names plus the period; the descriptor reads `service_shape =
"27:904:0:0:0:0:0:0:0"`. The peer was the pinned Agent (loopback-pinned) and a
stock Humble `ros2 service call /param_talker/set_parameters` naming the 25,
about 2.4 KB:

| build | result |
| --- | --- |
| main, default buffers | no reply (CLI killed). The image logged `request dropped by the TRANSPORT inbox ... zenoh: NROS_PARAM_SERVICE_INBOX_BYTES, or NROS_SERVICE_INBOX_BYTES` — zenoh's knobs on an XRCE image |
| main, `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE=4096`, custom MTU 512, history 4 | no reply, and a `get_parameters` that answered before got NO reply after: the session was wedged |
| same, history 16 | 25 of 25 `successful=True`, and `get_parameters` still answers |
| this fix, default knobs | 25 of 25 `successful=True` |

**Which limit binds.** Two of them do, and they bind in different ways:

1. **The request buffer** (`XRCE_SERVICE_REQUEST_BUFFER_SIZE`, default 1,024 B)
   binds first on every default build. An oversized request is flagged on
   arrival and logged.
2. **The reliable stream's window** (`XRCE_STREAM_HISTORY` slots of the custom
   transport MTU). The Agent fragments at the MTU the client reports, and the
   client reassembles only inside that window (`uxr_receive_reliable_message`).
   A message needing more fragments than the window holds never completes, and
   since the stream is reliable it blocks everything behind it. That is
   silent, and it takes the whole session down. It binds only on small-MTU /
   small-history builds; at the defaults (MTU 4096, history 16 or a derived 4)
   a 2.4 KB request is one fragment.

**The fix:**

- `NROS_PARAM_SERVICE_INBOX_BYTES` is bound to a new define,
  `XRCE_PARAM_REQUEST_BYTES` (`xrce-config.txt`). It is the backend-neutral
  statement nros-node already checks against the request it prices. Unstated,
  the cargo lane derives it from `[params] service_shape`
  (`XrceDemand::derive_param_request`). This happens before the endpoint-QoS
  gates, because it depends on the parameters only.
- `internal.h` RAISES the request buffer to it, never below the old floor. It
  `#error`s on a STATED buffer below it, and on any of the three receive
  buffers needing more fragments than the window can reassemble. These are C
  guards, so they bind every producer: env, Kconfig, the cargo derivation, a
  board rung.
- A declaration this crate cannot price (a string or array parameter) REFUSES
  the build and names `NROS_PARAM_SERVICE_INBOX_BYTES`, exactly as zenoh does
  (`1352-param-set-request-exceeds-service-buffer.md`).
- The arithmetic is shared, not copied (issue 1025): `param_request_max_from`
  moved from `nros-rmw-zenoh/build_param_slot.rs` to `nros-sizing-descriptor`,
  unfloored like the rest of that crate. Zenoh keeps its own floor and refusal.
- nros-node's inbox-overflow line names the XRCE knob as well.
- `check-xrce-config-manifest` read only `NROS_XRCE_*` names as forwarded, so
  it called the forwarded backend-neutral knob unwired. Its membership check
  now reads any `NROS_*`; the reverse check stays XRCE-scoped.

**Knob routes checked:** env (both lanes), Kconfig
`CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES` / `CONFIG_NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE`
(forwarded by `nros_cargo_build.cmake`, read by the Rust lane through
`$DOTCONFIG`), and the cargo derivation. The CMake lane is a CTest harness that
resolves env only (phase-420 W9), so it sees statements, not derivations, as
for the subscriber family.

**Tests:**

- Live cell `native-params-25-rust-xrce`
  (`params_per_node_interop::xrce_set_parameters_naming_25_declared_lands`):
  PASS, 25 × `successful=True`, 3.2 s solo. With main's XRCE sources it FAILS,
  no reply in 90 s.
- The `XrceDemand` selftest gains derive / stated / undeclared / refuse cases.
- `nros-sizing-descriptor` unit tests cover the shared price.
- Compile controls:
  - `NROS_XRCE_CUSTOM_TRANSPORT_MTU=512 NROS_XRCE_STREAM_HISTORY=4 NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE=4096`
    → `#error` (window);
  - `NROS_PARAM_SERVICE_INBOX_BYTES=2564 NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE=1024`
    → `#error` (stated short);
  - history 16 builds.

**Sweep:**
`git grep -n 'NROS_PARAM_SERVICE_INBOX_BYTES\|XRCE_SERVICE_REQUEST_BUFFER_SIZE\|XRCE_PARAM_REQUEST_BYTES\|param_request_max_from\|XRCE_STREAM_HISTORY' packages zephyr cmake`

## Not measured

- Any embedded XRCE image (Zephyr, FreeRTOS, serial). The guards are C and
  compile there; no RTOS image was run against a peer.
- A request larger than the buffer a build derived: a peer can always send
  one. It is still flagged and logged when it fits the window. Beyond the window
  it still wedges the session. That is upstream client behaviour (the window
  cannot drop a half-received reliable message), and only the build-time guard
  keeps a DECLARED request inside it.
- Replies. Outbound reliable writes fragment across free slots
  (`uxr_prepare_reliable_buffer_to_write`), so they are bounded differently;
  not measured here.

Filed while measuring: issue 1743 (open, `docs/issues/`).
The cargo road never
counts the param + lifecycle service servers for XRCE, so this very image could
not boot until its row stated `NROS_XRCE_MAX_SERVICE_SERVERS=11`.
