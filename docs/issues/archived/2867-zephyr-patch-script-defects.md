---
id: 2867
title: "Three Zephyr patch scripts: a dead 3.7 script, a self-test that fails at main, and a rewrite on every run"
status: resolved
type: bug
severity: low
area: [zephyr, build, tooling]
related: [issue-2831, issue-2764, issue-2407]
found: 2026-11-22
resolved: 2026-11-23
resolved_in: "the change that archives this issue"
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

## Fix

1. **Deleted** `scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh` and its line in
   `patches/3.7.sh`. Correcting the guard would not have revived it.
   `native-sim-ipproto-ip-patch.sh` writes the same arm one step earlier in
   the same set, so a working guard must always find the arm present.
   Measured against upstream v3.7.2's `drivers/net/nsos_adapt.c`: the one
   stock `case NSOS_MID_IPPROTO_IP:` is in `socket_proto_from_nsos_mid`, and
   `nsos_adapt_setsockopt` has an IPV6 arm only. The script never wrote, so
   deleting it changes no workspace. Its entries in
   `.config/zephyr-workspace-resolvers.txt` and `scripts/grep-q-baseline.json`
   (both ratchets) went with it. The upstreaming note in `book-internals` now
   says where the 3.7 host arm comes from. The 4.4 twin
   (`nsos-adapt-ipproto-ip-patch-4.4.sh`) is live, because 4.4's guest-side
   script does not write the host arm, and it is unchanged.
2. **`cargo-features-patch.sh --self-test`**: both fixtures now carry every
   line a hunk anchors on, in upstream's spelling. The shared (3.7) layout
   now has the `CARGO_ARGS build` / `CARGO_ARGS doc` callers (checked against
   zephyr-lang-rust `404fcef`), and both layouts have the real
   `if(CONFIG_DEBUG)` block the profile hunk anchors on. The test also asserts
   one staticlib-only build command and one profile override, and that the
   doc command keeps `doc`. Before: `patch FAILED on run 1`. After: OK.
   Negative control: the staticlib hunk mutated to drop `--crate-type
   staticlib` fails (`staticlib=0 ... want 1/1/1/1/1`).
3. **`tcm-section-align-patch-4.4.sh`** writes a file only when that file
   still has an edit to make, so an applied tree is not touched. New
   `--self-test`, run on stub copies of both files:
   - check mode on the pristine pair exits 1 and writes nothing;
   - a normal run patches both files;
   - check mode then exits 0;
   - a second normal run leaves both files' mtimes and bytes unchanged and
     says `already applied`.

   Negative control: the old unconditional write fails the test (`the second
   normal run rewrote an applied file`).

New gate `just check zephyr-patch-self-tests`
(`scripts/check-zephyr-patch-self-tests.sh`) runs both self-tests on the fast
line. No lane running defect 2's self-test is why it went unnoticed. On every
run, before the self-tests, the gate puts each one's defect back in a copy of
its script (the two negative controls above) and requires that self-test to
fail. So a self-test that can no longer fail is a red too.
