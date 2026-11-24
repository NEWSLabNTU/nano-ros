---
id: 2933
title: "A probe-run step ends at its expected output and never checks the command's exit status"
status: open
type: test-gap
area: [testing, book]
severity: medium
found: 2026-11-24
---

## What happens

A book step tagged `probe=NN probe-run=SECS` with a `probe-expect=NN` block
is run by `nros_probe_run_expect` (`scripts/probe/probe-run.sh`). That
helper polls the step's log for the expected lines. When every line has
printed, it stops, kills the step's process group, and passes the step. It
never waits for the command to finish and never reads its exit status.

That is the right shape for a command that runs forever: a talker, a QEMU
boot or a router, which the probe has to stop. It is wrong for a command
that finishes. If the expected text prints before the command is done, the
step passes even if the command would have failed after it.

This happened on the issue 2912 upgrade tracks (not merged). Their page
steps ran `nros configure --migrate` and `nros build` as `probe-run` steps.
One Rust run passed with the build cut off after 6 crates, before the
build's epilogue, where the pin-set drift check that refuses the build
runs. Every cold run after it refused. Those steps are now plain `probe=`
steps, which must run to the end and exit 0.

## Where else it applies

All 50 `probe-run` steps in `book/src` were read on 2026-11-24. 45 wrap a
command that runs until stopped (`nros run`, `ros2 topic echo`, the router,
an emulator, the PX4 daemon). These terminate on their own:

| Step | Command | Expected text |
| --- | --- | --- |
| `guide/build/install-options.md` probe=50 (pinned) | `nros pin` | the pinned release's manifest path |
| `guide/platforms/px4.md` probe=70 | `bin/px4-nano_ros_app start` | `nano-ros uORB backend registered` |
| `tutorial/qos.md` probe=2030 | `nros build` (meant to fail) | the `[qos-match]` refusal |
| `tutorial/tell-the-build.md` probe=855 | `nros build` (meant to fail) | the undeclared-parameter refusal |

In each of them the expected text is the last thing the command prints, so
none is known to pass a command that failed. The gap is that nothing checks
its status:

- `nros pin` could print the manifest path and still exit non-zero.
- The two refusal steps pass on the message alone. A build that printed
  the refusal and then exited 0 would pass.

The two refusal steps cannot become plain steps either, since a plain step
must exit 0.

## What would close it

Let a `probe-run` step say how it ends:

1. The default stays: a command that runs until stopped passes on its
   expected lines.
2. A new tag, for example `probe-exit=0` or `probe-exit=fail`, makes the
   helper wait for the command to exit within SECS. It then requires that
   status (0, or non-zero) and the expected lines in the full log.
3. The extractor refuses `probe-exit` together with `probe-keep`.
4. The four steps above take the tag: `probe-exit=0` for `nros pin` and
   `px4-nano_ros_app start`, `probe-exit=fail` for the two refusals.
5. Optionally, the extractor warns when a `probe-run` step's command is a
   known terminating verb (`nros build`, `nros setup`, `nros pin`,
   `nros configure`) without `probe-exit`.

## Related

- `scripts/probe/probe-run.sh` (`nros_probe_run_expect`)
- `scripts/probe/extract-book-steps.py` (the `probe-run` / `probe-expect` tags)
- issue 2912 (the upgrade tracks, where this showed)
