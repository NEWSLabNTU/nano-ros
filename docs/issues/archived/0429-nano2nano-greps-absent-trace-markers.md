---
id: 429
title: "nano2nano GID/sequence tests grep listener trace output the binary no longer emits"
status: resolved
type: bug
area: testing
related: [issue-0422, issue-0157, issue-0164]
resolved_in: nano2nano MessageInfo channel retarget
---

Grep-drift class (like 0157/0164). `test_sequence_number_increment` and
`test_gid_consistency` spawned the native Rust LISTENER with `RUST_LOG=trace` and
parsed `MessageInfo` seq/GID out of its stderr — but phase-277 slimmed the demo
listener (`create_subscription_for_callback_name`, no receive-side trace, no
`Waiting for`), so the greps found nothing ("got 0") and the readiness wait timed
out silently.

RESOLVED by observing the AUTHORITATIVE source instead: the zenoh publisher shim
logs the per-message MessageInfo it stamps into the wire attachment
(`nros-rmw-zenoh/src/shim/publisher.rs`: `… with attachment: seq=N, ts=…,
gid=[..]`). The two tests now run the pair with `RUST_LOG=trace` on the TALKER,
parse seq (monotonic per publisher) and gid (constant per publisher) from that,
and assert on `nros_tests::output::MESSAGE_INFO_{ATTACHMENT_MARKER,SEQ_PREFIX,
GID_PREFIX}` constants — not literals — so the next banner change breaks the
constant, not the grep (CLAUDE.md rule). If the marker is absent the test now FAILS
LOUDLY naming the drift instead of continuing into a confusing "got 0".

Parsers fixed for the real format (`seq=1,` — leading digits only; `gid=[c0, 3b,
8b, a3]` — the whole bracketed array, was truncating at the first space). Verified:
both pass (GID consistent `[80, b7, 08, 4a]`, seq `[1, 2]` monotonic).
