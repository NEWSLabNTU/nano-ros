---
id: 468
title: "`TransportError::InvalidConfig` had no ABI code, so a capacity the build cannot honour arrived as a caller error"
status: resolved
type: bug
severity: low
area: rmw, abi
related: [issue-0465, issue-0348]
resolved_in: phase-209
---

## Finding

`ret_from_error` encoded `TransportError::InvalidConfig` as
`NROS_RMW_RET_INVALID_ARGUMENT` (-4), and `error_from_ret` decoded -4 back to
`InvalidArgument`. The variant did not survive the C ABI.

That is the last hop of issue 0465's collision. After 0465 taught the zenoh
backend to report an exhausted session pool as `InvalidConfig`, the value still
reached the far side as `InvalidArgument` — "you passed something wrong" —
when the arguments were fine and the BUILD could not honour them. The two have
opposite remedies: `InvalidArgument` means fix the call, `InvalidConfig` means
rebuild (or stop asking for the extra resource).

## Resolution

`NROS_RMW_RET_INVALID_CONFIG = -19`, following the `-18 CONNECTION_FAILED`
precedent (phase 155.B.3), which was added for exactly this reason — so callers
could tell "I can't reach the router" from "internal backend invariant tripped".

* `packages/core/nros-rmw-abi/include/nros/rmw_ret.h` — the code + its contract.
  The C headers are the ABI SSoT (RFC-0054).
* `packages/rmw/cffi/src/lib.rs` — both directions of the mapping.
* `packages/rmw/cffi/src/generated.rs` — regenerated via
  `scripts/gen-abi-bindings.sh`; the diff is the single new constant.

Verified with a two-session probe against a live router, before and after:

```
before:  nros: RMW session open failed — InvalidArgument
         second open: Err(Transport(InvalidArgument))

after:   nros: RMW session open failed — InvalidConfig
         second open: Err(Transport(InvalidConfig))
```

Blast radius was three files: only `rmw_ret.h`, the generated bindings, and the
two mapping functions consume these codes.
