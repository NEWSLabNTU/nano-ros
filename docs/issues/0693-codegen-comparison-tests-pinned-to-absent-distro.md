---
id: 693
title: "`rosidl-codegen`'s comparison + parity suites read `/opt/ros/jazzy`, which this project does not install — 13 tests bail early and report PASS"
status: open
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

## Directions

- **Resolve the distro instead of naming one.** `$ROS_DISTRO` when set, else the
  single entry under `/opt/ros`, else skip. Keeps the tests working on humble,
  jazzy and a distrobox alike.
- **Make absence honest.** Fail, or skip through a mechanism the harness counts.
  A `return Ok(())` is neither. If `nros-tests` cannot be a dependency of a
  `packages/cli` crate, the sub-workspace needs its own one-line skip macro
  rather than a comment saying "skipping".
- **`check_parser_failures.sh` has the same pin** and should take the same
  resolution, or the script silently checks nothing.

Whichever way it goes: a suite that answers in 27 ms is reporting on work it did
not do, and that is the part worth fixing first.
