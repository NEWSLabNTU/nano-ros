---
id: 2490
title: "nros-cpp's action path still decides whether a payload has a CDR header by sniffing its bytes, the value-based check RFC-0069 removed from the executor"
status: resolved
resolved: 2026-11-19
resolved_in: "the change that archives this issue"
type: bug
severity: low
area: [cpp-api, actions]
related: [rfc-0069, 2524, phase-555]
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

## Merged from issue 2524 (2026-11-18)

Issue 2524 was filed for the same gap a day later, from the same phase-555
review. It is archived as a duplicate of this one. What it added:

- The function is `payload_has_cdr_encap`, called from `frame_for_callback`.
  It treats `00 {00|01|06|07|0a|0b}` as a header and forwards such a payload
  verbatim; otherwise it prepends a little-endian header.
- If some road does deliver a raw payload, frame it on that road, where the
  framing is known, rather than guessing in the trampoline.

## Resolved (2026-11-19)

Traced: `cpp_feedback_trampoline` and `cpp_result_trampoline` have one
caller, the executor's raw action-client dispatch
(`register_action_client_raw`). It hands them `splice_feedback` /
`splice_result` output: the frame's own encapsulation header followed by
the fields, on every backend. So the sniff's "no header" branch was
unreachable in practice, and its "header" branch was a copy.

- The trampolines forward the dispatch's buffer as it is. They no longer
  copy it into a stack frame or truncate it to `DEFAULT_RX_BUF_SIZE`.
- nros-cpp's `payload_has_cdr_encap` and `frame_for_callback` are deleted.
- The contract is stated where a callback author reads it:
  `RawFeedbackCallback` / `RawResultCallback` say the payload is a whole CDR
  message behind its header, spliced by the executor.
- Test: `action_frame`'s
  `a_body_that_reads_as_a_header_still_gets_the_frames_header`. A feedback
  or result body whose first word is 256 (`00 01 00 00`) still gets the
  frame's header in front, which a sniffing reader would have misread.
