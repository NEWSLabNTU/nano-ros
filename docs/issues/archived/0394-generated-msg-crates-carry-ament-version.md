---
id: 394
title: "Generated message crates carry the source package version, so a committed leaf `Cargo.lock` encodes WHICH interface source built it — and every other host reads drift"
status: resolved
resolved_in: c673faa40 + 93aa02016
type: bug
area: codegen
related: [issue-0359, issue-0378, issue-0386, rfc-0023, rfc-0048]
---

# Leaf locks encode the interface source, so they drift per host

## Symptom

`just build-test-fixtures` fails on a ROS-less host:

```
error: cannot update the lock file
  /…/packages/testing/nros-bench/executor-fairness/Cargo.lock
  because --locked was passed to prevent this
```

The lock is TRACKED, so `--locked` is correctly enforced (issue 0386). The delta
is one line:

```diff
 [[package]]
 name = "action_msgs"
-version = "1.2.2"
+version = "1.2.3"
```

## Cause

The generated crate's version is the version of whatever interface source
produced it:

- a host **with ROS 2 Humble** generates from `/opt/ros/humble` → `action_msgs`
  **1.2.2**, which is what the committed lock records;
- a host **without ROS 2** generates from the vendored
  `packages/cli/interfaces/action_msgs` → `<version>1.2.3</version>` → the
  generated `Cargo.toml` says **1.2.3**.

Both are correct codegen. The lock cannot be right for both, so it flips
depending on who last regenerated. The git history of this one file shows the
churn already:

```
19ba3454e chore: refresh the executor-fairness lockfile
322a8ebf3 Revert "fix(#359): refresh the executor-fairness leaf lock (fallout from aafd9e30b)"
5e0ffe099 fix(#359): refresh the executor-fairness leaf lock (fallout from aafd9e30b)
```

Refresh, revert, refresh — three commits fighting over a value that encodes the
committer's environment rather than the project's intent.

## This is the documented invariant, not a new theory

CLAUDE.md already states the rule and the reason:

> **Generated msg crates are the exception**: they are produced per host by
> `nros sync` from the consumer's ament install and never shipped, so codegen
> emits a CONSTANT `version = "0.0.0"` (the ament version moves to
> `[package.metadata.nros] ament_version`) — otherwise a committed lock asserts
> which ROS install built it and every other host reads as drift.

The rule is not in effect. Checked on this host:

```
$ grep -n '^version' …/executor-fairness/generated/action_msgs/Cargo.toml
3:version = "1.2.3"
$ grep -n '^version' …/executor-fairness/generated/std_msgs/Cargo.toml
3:version = "4.9.1"
$ grep -rn 'ament_version' packages/cli --include='*.rs'      # no hits
```

So generated manifests carry the ament version verbatim, no
`[package.metadata.nros] ament_version` key exists anywhere, and the predicted
consequence — "every other host reads as drift" — is exactly what blocks the
fixture build here. `0.0.0` appears in `cargo-nano-ros/src/package_xml.rs:72`
only as a fallback for a `package.xml` with no `<version>`, not as the constant.

## Why it bites harder now

Two changes made a latent inconsistency load-bearing:

- `--locked` is injected project-wide by the `scripts/bin/cargo` shim
  (issues 0359/0378), so a drifting tracked lock is now a hard build failure
  rather than a silent rewrite.
- The vendored interface set was completed (issue 0368 F4) so ROS-less hosts can
  generate at all — which is what makes the two sources, and therefore the two
  versions, both reachable.

The result: a contributor with ROS and a contributor without it cannot both build
fixtures from the same commit. One of them must commit a lock churn that breaks
the other.

## Direction

1. **Implement the documented rule**: emit a constant `version = "0.0.0"` for
   generated message crates and move the real version to
   `[package.metadata.nros] ament_version`. The consumer manifests already
   depend on these by path/patch, not by version, so the constant costs nothing
   at resolution time.
2. Regenerate the tracked leaf locks ONCE after that lands (via
   `just lock-update`, per the lockfile policy), after which they stop encoding
   the generator's environment.
3. Consider a gate: a tracked leaf lock naming a generated msg crate at anything
   other than `0.0.0` is drift by construction and can be grepped for.

Until then, the workaround on a ROS-less host is
`NROS_CARGO_FLAGS= just build-test-fixtures`, which lets the lock rewrite — and
leaves a modified tracked file that must not be committed.

## Evidence

Arch Linux, no ROS 2 (interfaces resolved from `packages/cli/interfaces/`),
2026-08-02, checkout `bdf76627a`. `NROS_CARGO_FLAGS= cargo build` in the leaf
succeeds and rewrites exactly the one version line shown above.

Filed as 0391 during an auth outage, when `just issue-new` could not claim the
reservation ref and fell back to local max + 1; another session had taken 0391,
so this is renumbered to 0394 — the collision the tool warned about.

## Resolved (2026-08-02)

`c673faa40` made the SECOND emitter obey the rule: `rosidl-bindgen`'s
`generate_cargo_toml` still wrote the ament version, while only the
`rosidl-codegen` jinja template emitted the constant — which is why RFC-0067
recorded "no codegen change is required for D2" and W3 nonetheless found 4.9.1
crates in the tree. `93aa02016` (phase-333 W1–W3) then converted the 25 committed
generated manifests and re-resolved 16 leaf locks, so a lock entry now records
`0.0.0` with no `source` line on every host.
