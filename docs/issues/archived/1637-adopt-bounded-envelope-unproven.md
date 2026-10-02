---
id: 1637
title: "`adopt-bounded` claims a doc comment states an envelope, and nothing
  checks that the comment exists or is true — 93 ledger rows make that claim"
status: resolved
type: tech-debt
area: [api, tooling]
severity: low
found: 2026-10-02
resolved: 2026-10-03
resolved_in: "branch issue-1637-adopt-bounded-envelope — `envelope` witness on adopt-bounded rows, ratcheted"
related: [1463, rfc-0089]
---

## The claim

RFC-0089's `adopt-bounded` means "same name and contract, weaker inside an
envelope that the DOC COMMENT states — the envelope is part of the API". It is
the one disposition that asserts something OUTSIDE the ledger. Measured
2026-10-02: **93 rows** carry it (79 `divergence`, 10 `declined`, 3 `rename`,
1 `gap`).

## Why it is ungated

`scripts/api-parity.py` validates that a disposition is one of the four words,
and since issue 1463 a `gap` on a declared name must name an `owed` witness
that is checked against the tree. Neither asks whether the envelope a row
cites exists. Issue 1463's own row (`c:log_severity_t`) showed it can be false
in the worst direction: `nros-c/src/log.rs` asserted the OPPOSITE of the
envelope the row said it stated, and `log.h` stated none.

## What 1463 did not do, and why

1463 priced this as candidate 1 and rejected it as THE fix for 1463, because
it catches a false envelope, not a closed gap. It is still a real gap of its
own. The shape that would close it is the one 1463 landed for `owed`: an
`envelope` object naming `{file, text}` that must be found in the tree,
required on `adopt-bounded` rows. Requiring it on all 93 at once fails the tree
the day it lands, so it needs the staging `--require-disposition` had — e.g.
validated when present, then required per shard as each is re-read.

## Resolution (2026-10-03)

**Shape: `"envelope": {"file", "text"}` on the row, which is #1571's witness
pointed the other way in time.** Same `{file, text}` object, validated by the
same `_validate_witness` and read by the same `witness_missing` that
`broken_witnesses` now also calls — one spelling, not two. An `owed` witness
exists because work is outstanding and the fix deletes it; an envelope must
outlive every edit. Both are red the moment their literal is gone. Meaning
lives in `docs/reference/api-parity-ledger/SCHEMA.md` "`envelope`";
`envelope` is refused on any disposition but `adopt-bounded`.

The shared reader also gained one rule, for both fields: the file must be
TRACKED (`git ls-files`). `os.path.isfile` passes an initialised submodule's
file and an untracked one, neither of which a fresh clone has.

**Staged as a ratchet, not a flip.** `.config/adopt-bounded-envelope-baseline.txt`
grandfathers rows with no envelope and may only shrink: a new `adopt-bounded`
row without an envelope is red, and so is a baseline line whose row gained one,
left `adopt-bounded`, or no longer exists. Checked by
`scripts/api-parity.py --self-test` on the fast line, which every `--check`
runs first.

**Negative controls, every run.** `--self-test` plants a shard file through
`load_ledger` and decides it with `envelope_findings`, the same function the
real ledger goes through: a new unwitnessed row, an envelope whose text is gone
(this issue's own `c:log_severity_t` case), one whose file is gone, and three
stale-baseline shapes are RED; a grandfathered row and a witnessed one are
green. Against the real tree: adding an unwitnessed `adopt-bounded` row to
`timer.json` failed the self-test, and so did deleting one baseline line.

**Backfill, measured.** 93 `adopt-bounded` rows (79 divergence, 10 declined,
3 rename, 1 gap — the count in this issue held). Each was re-read against the
tree; a witness was accepted only where the envelope THE ROW CLAIMS is written
where a porting user reads it (doc comment on the declaration, public header,
the C++ guide), file tracked, text on one line. No doc comment was edited to
hold a string. **49 backfilled, 44 remain in the baseline.** The 44 are not one
class, and the first two groups below are the defects this issue exists to
surface.

### Envelope not found anywhere a user reads (17)

- `cpp:CancelResponse`, `cpp:GoalResponse` — our enumerators are 0-based where
  rclcpp_action's are 1-based; nothing user-facing says so.
- `cpp:Server` — absent `Waitable` members / `SharedPtr` creation stated
  nowhere; `book/src/getting-started/porting-a-cpp-node.md` says "The action
  call shapes (send_goal_async etc.) match."
- `cpp:Server::publish_feedback`, `cpp:Server::succeed`, `cpp:Server::abort`,
  `cpp:Server::canceled` — goal-id + const-ref instead of goal handle +
  `shared_ptr`, never stated.
- `rust:Subscription` — callback bound at declaration, no later
  `set_callback`; stated nowhere.
- `rust:Parameters`, `rust:RmwParameterConversionError` — the rename from
  rclrs's name is in plain `//` facade comments only.
- `rust:Context` — missing `ok()` unstated; the caller-storage half is stale
  (`Context::create_executor` now exists).
- `rust:Logger::set_default_level`, `rust:Logger::unset_level` — `Severity`
  has `Trace` and no `Unset`, unlike rclrs's `LogSeverity`; the book instead
  calls `Severity` "matching rcutils_log_severity_t".
- `c:client_init_default` — the arg-3 type is stated on the server sibling only.
- `rust:Client::service_name`, `rust:Service::service_name` — the receiver
  split is unstated; the reachable docs state a different bound (`&str` vs
  `String`).
- `rust:NodeOptions` — build-time arguments / missing rosout unstated.

### The code contradicts the row (5)

- `cpp:Logger` — the row cites a header envelope "RCLCPP_* macros lose the
  logger NAME"; `log.hpp` now says "THE LOGGER IS CARRIED, NOT DISCARDED".
- `cpp:Client::wait_for_action_server` — the row says the timeout defaults to
  5000 ms; the header says that substitution was removed and the no-argument
  form is a `static_assert` refusal.
- `rust:Publisher` — the row says there is no loan API; `EmbeddedRawPublisher`
  has `try_loan`/`loan` and `nros-rmw` has `SlotLending` behind `lending`.
- `rust:Executor::spin_async` — the row says the doc states "no errors,
  always-ready CPU poll"; the doc says only "Runs forever, yielding between
  poll cycles", and `book/src/concepts/ros2-comparison.md` says it "wakes on
  RMW I/O".
- `cpp:Node::declare_parameters` — the Result-vs-`std::vector` bound is not
  stated, and `book/src/concepts/ros2-comparison.md` says `declare_parameters`
  is "not exposed", contradicting the shipped API.

### Partial, misplaced, or invisible to the reader (21)

A statement exists but does not carry the row's whole envelope, sits on a
different declaration, or is a plain `//` rustdoc does not render:
`cpp:Client::send_goal` (its doc block sits above `wait_for_action_server`, so
Doxygen attaches it to the wrong method), `cpp:Client::set_callbacks` (guide
says before-or-after `send_goal_async`, header says before),
`cpp:Server::set_accepted_callback`, `cpp:Server::complete_goal`,
`cpp:Node::create_subscription`, `cpp:Publisher::publish`,
`cpp:create_publisher`, `rust:MessageInfo`,
`rust:Node::get_publishers_info_by_topic`,
`rust:Node::get_subscriptions_info_by_topic`,
`cpp:LifecycleNode::declare_parameters`,
`cpp:LifecycleNode::undeclare_parameter`, `cpp:create_timer`,
`rust:Clock::new`, `rust:ParameterBuilder::default_from_iter`,
`rust:ParameterBuilder::default_string_array`, `rust:Timer`,
`rust:Context::from_env`, `cpp:Node::get_logger` (stated only in the C
header), `rust:Executor::spin` (row describes the retired `spin(Duration) ->
!`), `cpp:spin_until_future_complete` (the 10 ms quantum is an in-body
comment).

### Not re-read (1)

`rust:init_with_args` — a concurrent session owns that row.

### Noticed while backfilling, not changed here

`book/src/getting-started/porting-a-cpp-node.md` says `rclcpp::init`'s
"argc/argv ignored." while `nros.hpp` (the `cpp:init` envelope) says a
`--ros-args` argv aborts. `cpp:RosoutQoS`'s header says "nano-ros publishes
no" `/rosout`, true for C++ only now that Rust has an opt-in one. Several
rows cite drifted line numbers (`cpp:Rate`, `cpp:WallRate`, `cpp:create_timer`,
`cpp:Node::Node`).

