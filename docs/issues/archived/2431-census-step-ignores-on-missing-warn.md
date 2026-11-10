---
id: 2431
title: "The census build step refuses a missing census that `[census] on_missing = \"warn\"` waives, so the configure warns and the build fails"
status: resolved
type: bug
severity: high
area: [build, census, cmake, cli]
found: 2026-11-11
related: [phase-530, issue-2396, issue-2269]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)** (W6, the census step). Found by a local replay of the `tutorial-c` / `tutorial-cpp` probe tracks on `main` `84fd55cd3a`, at step 830 of the Tutorial's chapter 8 (`tell-the-build.md`, "Measure it again").

## Measured

Chapter 8 writes a contract beside the launch file
(`launch/system.contract.yaml`). The image has no host sibling, so no census
can be taken, and step 825 opts out of the refusal:

```toml
[census]
on_missing = "warn"
```

The configure's `ws entity-census check --require-fresh` honours it and
prints a warning: "census missing (WARNING, [census] on_missing = "warn") ...
This image's pools are derived from a contract that NOTHING has compared with
the code". Then the build-time census step (`nros_census_step`, target
`nros_census_step_*`, `take --model <m> --build-step`) asks the same question
again, without the policy, and fails:

```text
[  1%] nano-ros: census step (.../build/nros/models/sensor_node_bringup/system_model.yaml)
Error: the census `.../system_model.census.json` is stale (no census at `...`), and no
host build is recorded beside it to re-take it during a build.
```

`nros build` exits 2. The chapter's step piped `nros build 2>&1 | sed -n
's/^  artifact: //p'`, so the reader (and the probe) saw only the exit code.

Minimal repro, both measured: `nros new sensor_ws --board mps2-an385-freertos
--template talker --lang c`, a one-node `launch/system.launch.xml`, the
scaffold's contract moved to `launch/system.contract.yaml`, and the
`[census]` block above. `nros build` exits 2 on `main`; with the fix, 0.

The Rust road passes the same chapter (`tutorial-8`): a cargo image has no
census build step.

## Resolution (2026-11-11)

`take_model_in_build` reads `[census]` from the bringup's `system.toml`, the
file the configure's check reads, found from the model the same way
`discover_contract` finds the bringup. When there is no host build to
re-take the census from, it applies `on_missing` (no census) or `on_stale`
(a census older than its inputs, or incomplete):

- `warn`: print the same warning the check prints (`freshness_verdict`),
  write a constant digest naming the verdict (write if changed, since
  `nros_census_step` names the digest as a configure input), and let the
  build go on. Never `--fail-if-changed`: the digest describes no census.
- `refuse`, the default: the error above, unchanged.

Test: `a_census_the_system_waives_warns_in_the_build_step_too`. Measured on
the repro: the first `nros build` exits 0 with the warning, and a second
reconfigures and compiles nothing.

The chapter's two measurement steps (810 and 830) now `tee` the build to
`build/build.log` and read `artifact:` from the log, so a failed build shows
its error.

Sweep: `git grep -n 'policy.on_missing\|policy.on_stale\|freshness_verdict('
packages/cli/nros-cli-core/src/cmd/entity_census.rs`. `check` and the build
step are the two sites that judge freshness against a policy. `take --image`
takes a census rather than judging one.
