---
id: 448
title: "XRCE action client: goal accepted and result delivered, but no feedback line matches"
status: open
type: bug
area: rmw
related: [issue-0422, issue-0429, issue-0157]
---

## Symptom

`xrce_ros2_interop::test_ros2_action_xrce_client`, reproduced on three
consecutive retries:

```
ROS 2 DDS action server ↔ XRCE action client did not complete (233.6):
accepted=true got_feedback=false.
```

## What the flags imply

```rust
let accepted     = client_output.contains("Goal accepted");
let got_feedback = client_output.contains(&format!("{} [0, 1", ACTION_FEEDBACK_PREFIX));
```

with `ACTION_FEEDBACK_PREFIX = "Next number in sequence received:"`.

The output is collected by waiting for `ACTION_RESULT_PREFIX` ("Result
received:") with a 20 s timeout and `unwrap_or_default()`. On timeout that
yields an EMPTY string, which would make `accepted` false too — so
`accepted=true` proves the wait **succeeded** and the terminal result line
arrived.

So the client got its goal accepted AND its result, and only the feedback
assertion failed. That narrows it considerably: the send_goal request/reply and
the get_result path both crossed XRCE→agent→DDS successfully.

## Two candidate causes, not yet separated

1. **Feedback genuinely does not arrive.** The feedback TOPIC (as distinct from
   the two services) has its own UUID framing; that path could be broken while
   the services work.
2. **Feedback arrives but does not match the literal.** The assertion requires
   `"Next number in sequence received: [0, 1"` — an exact prefix INCLUDING the
   first two Fibonacci elements and the space after the comma. Any of: a
   different formatting of the sequence, feedback that starts later in the
   sequence, or a partial first message, makes a working feedback path read as
   failure.

Candidate (2) is the same grep-drift class as #0429 and archived #0157/#0164,
where a slimmed example's output silently broke greps that had encoded its exact
text. That class has already produced two false diagnoses in this tree, so it
must be ruled out before the RMW path is suspected.

## First step

Print the client output on failure — the message already interpolates
`{client_output}`, so capture a real failing run and read what the feedback
lines actually say. If feedback lines are present with different text, it is (2)
and the fix is to assert on the shared constant plus a NUMERIC check rather than
a literal prefix of the payload. If there are no feedback lines at all, it is
(1) and the feedback topic is the subject.

## Notes

Found triaging #0422. The test retries 3× and fails identically each time, so it
is deterministic rather than a timing flake — which also argues against a
race in the feedback path and mildly toward (2).
