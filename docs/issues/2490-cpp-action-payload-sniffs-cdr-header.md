---
id: 2490
title: "nros-cpp's action path still decides whether a payload has a CDR header by sniffing its bytes, the value-based check RFC-0069 removed from the executor"
status: open
type: bug
severity: low
area: [cpp-api, actions]
related: [rfc-0069]
found: 2026-11-13
---

**Found by** the phase-555 W2 review of RFC-0069, item B10 in
`docs/roadmap/data/phase-555-w2-rmw-wire-brief.md`.

## Measured

- RFC-0069 settled the action payload envelope as one CDR header, ROS 2's
  own. The executor now always splices the header back
  (`packages/core/nros-node/src/executor/arena.rs:3474-3498`). Its old sniff,
  which guessed from the first bytes whether a header was present, is
  `#[cfg(test)]`-only (`arena.rs:3462`). RFC-0069 recorded that sniff as a
  live bug, because a payload whose first bytes happen to look like an
  encapsulation header is misread.
- nros-cpp still has a non-test `payload_has_cdr_encap`
  (`packages/api/nros-cpp/src/action.rs:1042-1059`) on its action path.

## Not verified

Whether any live caller can hand that function a header-less payload. The
reviewer believes upstream always splices a header, which would make the
sniff unreachable in practice, but did not trace every caller.

## Next

Make the C++ path splice unconditionally, as the executor does, and delete
the sniff. Add a test with a payload whose leading bytes equal an
encapsulation header (`00 01 00 00`), which the sniff would misread.
