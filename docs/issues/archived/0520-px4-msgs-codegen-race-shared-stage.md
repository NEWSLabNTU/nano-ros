---
id: 520
title: "px4_msgs codegen races itself: 87 parallel compile-check units regenerate into the same three output dirs, and the error names a source file that exists"
status: resolved
type: bug
severity: high
area: build, px4, codegen
related: [issue-0510, issue-0034, phase-319, phase-336]
resolved_in: "issue-0520 (advisory lock around the shared px4_msgs codegen)"
---

## Symptom

`just build-test-fixtures lane=all` fails with rc=2 AFTER every platform lane has
reported OK — the failure is in the compile-check stage:

```
Error: stage /home/aeon/repos/nano-ros/third-party/px4/PX4-Autopilot/msg/GpsDump.msg

Caused by:
    No such file or directory (os error 2)

Location:
    rosidl-bindgen/src/generator.rs:711:18
   px4_msgs codegen FAILED for px4_stub (no stamp)
```

The named file exists. So do the other fourteen — **15 distinct `.msg` names**
were reported missing in a single run, out of 201 present on disk, with the PX4
submodule clean and at its recorded pin (`d6f12ad1`). A missing-file error that
names files which are all present is the tell.

## Cause

`scripts/build/compile-check-fixtures.sh` is invoked **once per compile-check
unit** — 87 of them in a `lane=all` run, under the jobserver pool, in parallel.
Every invocation runs the same px4 block, regenerating px4_msgs into the same
three leaf dirs:

```
examples/px4/rust/companion/px4-probe/generated
examples/px4/rust/companion/px4-stub/generated
examples/px4/rust/companion/offboard-companion/generated
```

`stage_px4_msgs` (`rosidl-bindgen/src/generator.rs`) stages the whole `.msg`
tree through `<output>/.px4_msg_stage` and `remove_dir_all`s it on every exit
path — success, `bail!`, and the caller's cleanup. Concurrent invocations
therefore delete each other's staging directory mid-copy. The failing
`std::fs::copy(&path, stage_msg.join(...))` reports `ENOENT` for the
*destination*, but its `wrap_err_with` prints the **source** path, which is why
the message accuses a file that is plainly there.

Which `.msg` loses the race is timing, hence a different name each run.

## Why it appeared only now

The px4 compile-check units only run in the FULL lane, and `lane=all` has not
completed in a long time — it was blocked first by the mixed-workspace link
failure (#0500) and then by the missing `nros sync` (#0510). Each fix uncovered
the next lane in sequence; this is the third.

## Fix

A repo-level advisory lock (`build/px4-msgs-codegen.lock`) around the codegen
call, the same idiom and reasoning as the zephyr fixture build lock in
`zephyr-fixture-make-driver.sh`. flock-absent hosts skip it, best-effort.

Scoped deliberately to the codegen CALL, not the surrounding loop: the
`cargo check` per leaf touches no shared staging and is the long pole, so
holding the lock across it would serialize 87 units on nothing. (The first cut
did exactly that and was corrected before running.)

## Better fixes not taken

The lock treats the symptom. Two deeper options, either of which would make the
lock unnecessary:

* **Make the staging dir unique** — `.px4_msg_stage.<pid>` or a tempdir — so
  concurrent generators cannot collide at all. This is the right fix and belongs
  in `rosidl-bindgen`, but it changes a shared codegen path used well beyond the
  compile-check stage.
* **Generate once, not 87 times.** The three leaves' `generated/` trees are
  identical across units and depend only on the PX4 submodule; regenerating them
  per unit is pure waste even when it does not race.

Also worth fixing on its own: `wrap_err_with` should name the DESTINATION on a
copy failure, or both paths. "No such file or directory" pointing at an existing
source cost real time here.
