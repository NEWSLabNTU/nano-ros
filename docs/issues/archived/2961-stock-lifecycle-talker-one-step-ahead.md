---
id: 2961
title: "lifecycle_cli_answers_as_stock_lifecycle_talker::case_1_c: the stock lifecycle_talker is sometimes one transition ahead of the CLI"
status: resolved
type: bug
severity: low
area: [testing, lifecycle, interop]
found: 2026-11-25
resolved: 2026-11-26
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

## Cause (2026-11-26)

The harness asked the stock side twice. `GraphSide::query`
(`nros_tests::ros2_compat`) ran a `ros2` command again, up to three times,
whenever the CLI's own middleware session failed to close ("Failed to close
the session"). That retry is right for a read, whose second answer is the
same. A `lifecycle set` is not a read: the first run had already moved the
node, and the second asked for a transition the node had just taken.

Both recorded reds are exactly that second answer:
- **`set activate`.** The first run activated the stock node. The retry found
  it active and was offered only `deactivate [4], shutdown [7]`.
- **`set cleanup`.** The first run cleaned up. The retry found the node
  unconfigured and was offered only `configure, shutdown`.

A teardown failure is the CLI process's own (a zenoh session close under
load), so it appears on either side at random. That is why the case and the
step varied, and why it passed solo. `once()` was not the retry; `query()`
under it was.

## Fix

- **A command that changes the node runs exactly once**
  (`ros2_compat::changes_the_node`): `lifecycle set`, `service call`,
  `param set`/`delete`/`load`, `action send_goal` and `topic pub`. A read is
  still asked again after a teardown failure.
- **The exit status is not compared when a teardown failed.** When either
  side's CLI failed to close its session, `Compared::diff` compares the two
  answers but not their exit statuses, which then report the teardown, not
  the node. The teardown line itself was already left out of the
  normalised text as the CLI's own log.

Tests: `only_a_read_is_asked_again`, and
`a_teardown_failure_compares_the_answer_not_the_exit_status`. The second
case is the measured red: the stock CLI printed "Transitioning successful",
then failed to close and exited 1, against nano-ros's 0. That now agrees,
and a different answer is still reported. The C case passed 3/3 on host 139
with the fix. That is only a sanity check, since the race is rare under no
load.
