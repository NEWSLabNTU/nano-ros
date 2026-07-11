---
id: 175
title: "Zephyr (native_sim) Cyclone action/service: server receives but the result round-trip never completes"
status: open
type: bug
area: rmw
related: [issue-0164, issue-0157, phase-286]
---

## Problem

On Zephyr native_sim CycloneDDS, a **service/action server receives the
request/goal but the client never completes** — the result/completion round-trip
is lossy. Discovery + request delivery work; the reply path does not.

## Evidence (2026-07-09 family re-run)

- `test_zephyr_dds_{c,cpp,rs}_action_e2e` —
  `server_received_goal=true, client_completed=false`. The goal reaches the
  server; the client never sees the result.
- `test_zephyr_cpp_service_server_to_client_e2e` — `client OK=1` of 3 expected:
  the client got at least one reply (so the basic reply path CAN work) but the
  run is short of the expected count. (The server-side `"Request"` marker was
  itself stale — fixed in the #164 sweep to `SERVICE_INCOMING_REQUEST_MARKER` —
  so the diagnostic now reads true instead of "server requests=0".)

phase_118 covers only pub/sub, so these action/service lanes have had no
recent last-known-good on fresh images — the completion gap could be a genuine
regression or long-standing.

## Narrowing (phase-286 W4, 2026-07-10)

Reproduced `dds_rs_action_e2e` with `--no-capture`. Precise picture:

- **Server side is fully green**: `Received goal request with order 1` → `Executing
  goal` → `Publish feedback` → `Goal succeeded`. Same baked domain (52) both sides.
- **Client side**: `Sending goal` → `Goal accepted by server, waiting for result`
  → then NOTHING for the full 90 s wait. It gets the immediate goal-accept reply
  but never the feedback (topic) nor the delayed `get_result` reply → never prints
  `ACTION_RESULT_PREFIX`.
- So client→server (goal request) AND the FIRST server→client reply (goal-accept,
  written right after the request while the client's reply-reader is freshly
  matched) both work. The LATE server→client paths — feedback (published at
  execute time) and the `get_result` reply (written ~8 s later on completion) —
  do not reach the client.

**Ruled out:**

- **NOT the #171/0171 VOLATILE-write-timing race.** That fix landed and was
  extended to all three writers: `service.cpp` has `request_writer_matched` +
  `wait_for_request_match` + `maybe_flush_request` (client request) AND
  `service_send_reply` gates on `dds_get_publication_matched_status.current_count`;
  `publisher.cpp` `writer_matched` gates the feedback publish the same way ("emits
  valid wire data once at least one reader has matched … VOLATILE `dds_write` into
  an empty pub-set is silently dropped"). So the writers already wait for a match.
- **NOT a stale marker** (unlike the #174 action lanes) — the client genuinely
  prints only up to "waiting for result".
- **NOT the dynamic-thread `tid … is in use!` warnings** — those appear on BOTH
  sides at `session_open` (Zephyr `kernel/dynamic.c` stack-free cleanup race) and
  the server works despite them.

**Trace-level narrowing (2026-07-11) — the writes ARE matched; the samples don't
arrive at the client readers.** Instrumented the SERVER's reply + feedback writers
(temporary `LOG_INF` in `service.cpp::service_send_reply` +
`publisher.cpp::publisher_publish_raw`, since reverted) and re-ran
`dds_rs_action_e2e`. Every server write went to a **matched** reader:

```
svc_send_reply seq=0 ready=1 cur=1 tot=1            # goal-accept reply
pub type=…Fibonacci_FeedbackMessage_ cur=1 tot=1    # feedback
svc_send_reply seq=0 ready=1 cur=1 tot=1            # get_result reply
```

So it is **NOT** a write-timing / match-gate / QoS problem — `current_count = 1`
on every write, the writer sees the client's reader, and `dds_write` runs. Also
ruled out: **NOT a spin/dispatch gap on the client** — `action_client_raw_try_process`
(`nros-node/src/executor/arena.rs:1219`) is driven every `spin_once` and DOES poll
feedback (`try_recv_feedback_raw`, :1260) and the `get_result` reply
(`try_recv_reply_raw`). The `tid … is in use!` dynamic-thread warnings are benign
(both sides; server works). Bumping `NROS_CYCLONE_MATCH_TIMEOUT_MS` to 30 s does not
help.

**RESOLVED to the layer (client-side read trace, 2026-07-11) — the transport +
rmw WORK; the bug is ABOVE them, in the nros action-client dispatch (and a
premature `get_result` reply).** Instrumented the CLIENT's read paths
(`subscriber.cpp::subscriber_try_recv_raw` + `_sequence`,
`service.cpp::take_typed_wire` + the reply correlation check in
`service_try_recv_reply_raw`; all temporary, reverted) and re-ran. The client
DOES receive everything at the rmw layer:

```
sub_take  type=…Fibonacci_FeedbackMessage_    matched=1 taken=1   # feedback received
reply_take type=…Fibonacci_GetResult_Response_ matched=1 taken=1   # result reply received
reply_corr got_seq=0 got_guid=… pend=0 my_guid=… match=1           # correlation MATCHES
```

So: the feedback sample is taken (`taken=1`), the `get_result` **response** is taken
(`taken=1`), and its correlation header matches the client's pending request
(`match=1`) — i.e. `service_try_recv_reply_raw` returns the reply successfully to
the action-client core. The goal-accept reply is likewise received and DOES reach
the app (`on_goal_response` → "Goal accepted"). **Yet `on_feedback`
("Next number in sequence received") and `on_result` ("Result received") never
fire, and the test fails.** So the loss is strictly between the rmw take (works) and
the app callback in
`nros-node/src/executor/arena.rs::action_client_raw_try_process` (steps 2 =
feedback, 3 = result; step 1 = goal-response works).

**Second, likely-related finding:** the `get_result` response is received at CLIENT
clock **04.339**, but the server does not print "Goal succeeded" until SERVER clock
**08.385** (~2 s later) — the server replies to `get_result` **before the goal
terminates** (a premature reply). service.cpp then clears `pending_seq` on that
early match, so the client stops waiting and the true terminal result is never
delivered. Fix direction is therefore two-fold, both ABOVE the cyclone rmw:
1. **Action server** (`nros-node` action_core) must hold the `get_result` reply
   until the goal reaches a terminal state (don't reply early), and
2. **Action client dispatch** (`arena.rs::action_client_raw_try_process`) must
   actually deliver the taken feedback + result to `on_feedback` / `on_result`
   (verify the `try_recv_feedback_raw` / `try_recv_get_result_reply` return values
   + the `FEEDBACK_PAYLOAD_OFFSET` / `RESULT_PAYLOAD_OFFSET` length gates).

Cyclone transport + `service.cpp` reply routing are proven working and are NOT the
cause. This is a nano-ros action-layer bug, likely not native_sim-specific.

## Suspects / direction (superseded by the narrowing above)

- The Cyclone reply/result writer→reader path on native_sim (NSOS sockets):
  service response topic, action `get_result` / feedback / status. Pub/sub works
  on the same fixture, so it is specific to the request-response entities.
- Actions never complete at all (0), service partially (1/3) — check whether the
  action result service is even discovered, and whether the service reply is
  dropped under load or lost to a transient-vs-volatile QoS mismatch (the #157
  class was ROS-form type names + domain collisions; this is past that).
- Bake distinct domains per role-set if not already (the #161 domain work) and
  confirm the result reader is declared before the server replies.

## References

`packages/testing/nros-tests/tests/zephyr.rs`
(`test_zephyr_dds_*_action_e2e`, `test_zephyr_cpp_service_server_to_client_e2e`),
issue #164 (re-triage), issue #157 (the earlier zephyr-cyclone service fix this
builds on), `packages/dds/nros-rmw-cyclonedds/`.
