---
id: 2468
title: "`bootstrap.sh` builds no runner package, so the checkout road's first `nros run` refuses"
status: resolved
type: bug
severity: medium
area: [install, cli, ux]
related: [phase-527, phase-523]
found: 2026-11-12
resolved_in: "fix-2468 branch: one list of what builds beside the CLI, read by setup-cli and bootstrap.sh; check-cli-companion-packages; nightly zenoh-probe"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md).** Found walking the book (`PROBE_TRACK=zenoh just probe bootstrap`, main `1df1191957`). CI never runs that track; the nightly bootstrap probe runs `quickstart` only.

## Symptom

The book's checkout road (book-internals `orientation/contributing.md`: clone, `./scripts/bootstrap.sh`, `source ./activate.sh`, `nros setup`) builds, then the first `nros run` refuses:

```
nros run: runner `native` (package `nros_runner_native`) is not built (looked for /root/nano-ros/packages/runners/target/release/nros-runner-native). Build it with `just setup-cli` in /root/nano-ros
```

That page says `just` is not a prerequisite, and the container has none.

## Cause

phase-527 W3/W5 moved the runners and the `router` / `px4` verbs into their own packages, each its own workspace. `just setup-cli` builds them beside the CLI. `bootstrap.sh`, the `just`-free front door, still builds only `nros` and `nros-launch-resolve`, and so does the page's by-hand block.

## Fix

The class, not the site: the set of host packages built beside the CLI is ONE
list, `scripts/lib/cli-companion-packages.txt` (`packages/runners`
`--workspace`, `nros_zenoh_tools`, `nros_px4_tools`). `just setup-cli` and
`bootstrap.sh` both read it; bootstrap fails loudly on a build error, like its
resolver step. The by-hand block in `contributing.md` gains the same three
lines. `just check cli-companion-packages` (fast tier) refuses:

- a reader that spells a listed manifest itself;
- a by-hand block that misses a line;
- a runner or command package (`<nano_ros_provides kind="runner"|"command">`,
  outside test fixtures) whose cargo workspace is not listed.

So the next such package fails a gate rather than a user's checkout. Negative
controls: dropping `nros_px4_tools` from the list, and main's `bootstrap.sh`,
each fail it.

The zenoh track gets a CI lane: a `zenoh-probe` job in `nightly.yml`, beside
`bootstrap-probe` and `installed-probe`.

**Acceptance (2026-11-12):** `PROBE_TRACK=zenoh just probe bootstrap` passes on the fix branch (`7e84fb3d8d`). Both interop verdicts pass: the talker published through the documented router, and stock `ros2 topic echo` received it. Main `1df1191957` failed the same track at the first `nros run`. The run took 59 min cold.
