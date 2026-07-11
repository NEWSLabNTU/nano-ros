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

**Precise remaining question:** the server writes goal-accept, feedback, AND the
`get_result` reply — all to a matched reader — and the client's spin polls all three
readers, yet only the **goal-accept** lands; the later **feedback** (topic) and
**`get_result` reply** never appear in the client's readers (`try_recv_*` return
empty, callbacks never fire). So it is a **selective receive-side delivery gap** on
native_sim NSOS: the FIRST server→client reply is received, the LATER ones are not.
Distinguishing "sample never reaches the client's reader cache" (RTPS/NSOS
transport) vs "reaches the cache but `dds_take` misses it" (reader instance/state)
needs a **client-side** trace (instrument `subscriber.cpp` take + the service
reply reader take, or `NROS_CYC_TRACE` the client's read path) — the next step.

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
