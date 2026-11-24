---
id: 2961
title: "lifecycle_cli_answers_as_stock_lifecycle_talker::case_1_c: the stock lifecycle_talker is sometimes one transition ahead of the CLI"
status: open
type: bug
severity: low
area: [testing, lifecycle, interop]
found: 2026-11-25
related: []
---

## What happens

`lifecycle_pair_interop`'s `lifecycle_cli_answers_as_stock_lifecycle_talker`
asks the nano-ros node and a stock `ros2 run lifecycle lifecycle_talker` the
same `ros2 lifecycle` questions in lockstep. Twice on host 241 its C case
failed because the STOCK node had already taken the transition being asked:

| Run | Step (`assert_same` label) | nano-ros answered | stock answered |
| --- | --- | --- | --- |
| PR #4795 cells, 2026-11-24 | `set activate` | Transitioning successful | Unknown transition requested, available ones are: deactivate [4], shutdown [7] |
| PR #4811 cells, 2026-11-25 | `set cleanup` | Transitioning successful | Unknown transition requested, available ones are: configure, shutdown |

In both, every earlier step agreed, so the stock node moved one transition
ahead between the previous step and this one. The C++ case passed the same
sequence each time, and #4795's C case passed solo 3 times out of 3. No
orphan processes were running on 241 for the #4811 run.

## What it is not

- A shared peer: each side has its own `rmw_zenohd`
  (`ZenohRouter::start_unique`, multicast scouting off), so the two cases
  cannot reach each other's `/lc_talker`.
- A retried `set`: the step queries each side once (`once()` is
  `ask_until(|_| true)`).

## Where to look

Something applies a transition to the stock node that the test did not ask
for, or applies a `set` twice. Candidates: the `ros2` CLI's own retry when a
reply is slow under load, or a daemon-backed query (`--no-daemon` is the
rule here, issue 1333) reaching a stale graph. The first step is to log each
stock `set`'s output and latency, and the stock node's own transition log,
which the harness already captures (`GraphSide::describe`), on the next red.
