---
id: 2867
title: "Three Zephyr patch scripts: a dead 3.7 script, a self-test that fails at main, and a rewrite on every run"
status: open
type: bug
severity: low
area: [zephyr, build, tooling]
related: [issue-2831, issue-2764, issue-2407]
found: 2026-11-22
---

Found while giving every patch script a check mode (issue 2831), not fixed
there.

## 1. `nsos-adapt-ipproto-ip-patch.sh` (3.7) can never write

Its guard, `grep -q "case NSOS_MID_IPPROTO_IP"`, also matches
`case NSOS_MID_IPPROTO_IPV6:`, which is in its own anchor. Stock 3.7 also
carries `case NSOS_MID_IPPROTO_IP:` in `socket_proto_from_nsos_mid`. So the
guard is always true and the script always skips. Its edit, an IPPROTO_IP arm
in `nsos_adapt_setsockopt`, is already made earlier in the set by
`native-sim-ipproto-ip-patch.sh` (phase 177.1 found that writing it a second
time broke every cyclonedds-zephyr fixture with `duplicate case value`). It is
dead code that reads as a live edit.

## 2. `cargo-features-patch.sh --self-test` fails at `main`

    self-test: shared layout: patch FAILED on run 1

The self-test's 3.7 ("shared") fixture has no `CARGO_ARGS build` caller line.
Upstream's 3.7 `CMakeLists.txt` has one, and the staticlib-only hunk (issue
2407) refuses a file without exactly one build command. So the fixture, not
the patch, is wrong. No lane runs the self-test, which is why nothing noticed.

## 3. `tcm-section-align-patch-4.4.sh` rewrites both files on every run

When the edits are already applied, a normal run writes the same text back to
`linker.ld` and `linker.cmake`. The content is unchanged, but the mtime moves,
so every build that depends on the linker script reconfigures for nothing.
Check mode does not write (issue 2831).

## Done when

- The 3.7 set runs no script that cannot write.
- `cargo-features-patch.sh --self-test` passes. A mutation of the patch makes
  it fail, and some lane runs it.
- A second normal run of `tcm-section-align-patch-4.4.sh` on an applied tree
  leaves both files' mtimes unchanged. A self-test shows it, with a negative
  control.
