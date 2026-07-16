---
id: 223
title: "action goal/cancel/result response parsers swallow CDR read errors via unwrap_or — truncated frames become plausible business values"
status: open
type: bug
severity: medium
area: core
related: []
---

## Finding (deep audit 2026-07-17, I3)

`packages/core/nros-node/src/executor/handles.rs:~2994` —
`parse_goal_accepted` / `parse_cancel_response` / `parse_result_response`
use `unwrap_or(default)` on `CdrReader` field reads: a malformed or
truncated wire response silently reports "goal rejected" /
`CancelResponse::default()` / `GoalStatus::default()` instead of an error.

## Fix sketch

Propagate the read Results (`.map_err(|_| NodeError::…)?`) so corrupt frames
surface; audit the sibling parsers in the same file for the pattern.
