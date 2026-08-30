---
id: 693
title: "`rosidl-codegen`'s comparison + parity suites read `/opt/ros/jazzy`, which this project does not install — 13 tests bail early and report PASS"
status: resolved
type: bug
area: testing/codegen
related: [issue-0686, issue-0683]
---

## Symptom

```
$ cargo nextest run -p rosidl-codegen --test comparison_test --test parity_test
Summary [0.027s] 19 tests run: 19 passed, 0 skipped
```

Nineteen tests, twenty-seven milliseconds, everything green. They are not
running: 13 of them read message definitions from a hardcoded
`/opt/ros/jazzy/...`, find nothing, and return `Ok(())`.

```
$ ls /opt/ros
humble
$ ls /opt/ros/jazzy
ls: cannot access '/opt/ros/jazzy': No such file or directory
```

`DEFAULT_ROS_DISTRO` in `nros-tests` is **humble**, and
`/opt/ros/humble/share/std_msgs/msg/Bool.msg` is present — so the inputs exist,
under the distro the project actually uses. Only the path is wrong.

## Why they pass instead of skipping

Both files bail with the documented anti-pattern:

```rust
// comparison_test.rs
Err(e) => {
    eprintln!("Skipping test: {}", e);
    return Ok(());
}

// parity_test.rs
if !Path::new(ros_share).exists() {
    eprintln!("Skipping test - ROS not found at {}", ros_share);
    return Ok(());
}
```

CLAUDE.md names this exactly — "Bare `eprintln!`+`return` reports PASS — never"
— and `nros_tests::skip!` exists for it. `rosidl-codegen` lives in the
`packages/cli` sub-workspace and does not depend on `nros-tests`, so the honest
spelling was not reachable from here; that is a reason it happened, not a reason
to keep it.

## Scope

| file | tests | hardcoded `jazzy` refs |
| --- | --- | --- |
| `tests/comparison_test.rs` | 4 | 1 (shared helper) |
| `tests/parity_test.rs` | 9 | 9 |
| `scripts/check_parser_failures.sh` | — | 1 |

11 references in total. The parity suite is the one that compares nano-ros
codegen against the reference `.msg` definitions — the property nobody has
measured on this host, on any run, for as long as the pin has been wrong.

## Fix

**Resolve the installed distro; never name one.** `parity_helpers::ros_share_root()`
takes `$ROS_DISTRO` when it points at a real tree, else the sole entry under
`/opt/ros`. With several installed and no `$ROS_DISTRO` it returns `None` rather
than guessing — ambiguity is not resolved by picking. Both test files and
`check_parser_failures.sh` go through it (the script now also EXITS NON-ZERO
when there is no ROS, instead of reporting "No msg directory" for every package
and succeeding).

**The early return is still there, and that is deliberate.** `cargo test` has no
runtime skip, and this crate is in the `packages/cli` sub-workspace which
`check-cli-tests` runs with a plain `cargo test` — no junit, so
`nros_tests::skip!` would be a hard failure rather than a skip. So a ROS-less
host still bails; what changes is that it says `[NO-ROS] <test>: ... did not
run` instead of "Skipping test", and it can no longer be confused with a wrong
path.

**The guard that makes that safe:** `ros_discovery_is_not_silently_broken` fails
whenever a ROS install EXISTS under `/opt/ros` but discovery returns `None`, or
returns a root with no `std_msgs/msg/Bool.msg`. "No ROS" stays quiet; "discovery
regressed" does not. That distinction is the one this issue is about — the old
code could not make it, which is why a jazzy pin survived on a humble host.

## Verified

- 27 tests run, 27 pass, and `--no-capture` shows **zero** `[NO-ROS]` markers —
  every test reached real message definitions under `/opt/ros/humble`.
- Adversarial check: forcing `ros_share_root()` to `None` on this host makes the
  guard FAIL and cancels the run, rather than letting 15 tests bail into green.
  Probe removed after the check.
- `check_parser_failures.sh` prints `using ROS share: /opt/ros/humble/share`.
- `just check cli-tests` and `just check cli-clippy` green.
