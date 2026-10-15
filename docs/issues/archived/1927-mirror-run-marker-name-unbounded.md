---
id: 1927
title: "`mirror-generated-header.sh` names its run marker after the whole output path: a long `TMPDIR` makes a copy-out build fail with `File name too long`"
status: resolved
type: bug
area: [build, cmake, testing]
severity: medium
found: 2026-10-16
related: [1911, 1925, 0834]
---

## Measured

`scripts/build/mirror-generated-header.sh` records each run in a marker
(issue 1911) at

```
<build>/CMakeFiles/nros-mirror-ran/<dest with every / replaced by _>
```

The marker is ONE path component, and its length is the length of the
absolute destination path, which is unbounded: it grows with wherever the
build directory happens to live. Linux caps a component at `NAME_MAX` = 255
bytes, so any build directory deep enough makes the `: >"$marker"` at the
end of the script fail, after the header was already copied, and the
mirror command exits non-zero.

Reproduced on `origin/main` (9cf11082a0) in a linked worktree:

```
TMPDIR=/tmp/claude-1000005/-mnt-evo-aeon-nano-ros/<session-uuid>/scratchpad/ppaths-tmp \
  scripts/check-template-copy-out.sh cpp-port-minimal-publisher
```

```
cpp-port-minimal-publisher/mps2-an385-freertos: FAIL — the copy does not build
FAILED: [code=1] nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h …
mirror-generated-header.sh: line 312: …/build/CMakeFiles/nros-mirror-ran/_tmp_claude-1000005_-mnt-evo-aeon-nano-ros_…_nros_cpp_config_generated.h: File name too long
check-template-copy-out: FAILED — a template a user copies does not build.
```

The two marker names in that run are 280 and 283 bytes long (the
`rclc/detail/nros_config_generated.h` and `nros/nros_cpp_config_generated.h`
mirrors). With the default `TMPDIR=/tmp` the same names are about 175 bytes,
which is why nothing noticed: the gate passes on a default host and fails
for anybody whose temp dir, CI workspace or build root is ~80 bytes deeper.
Agent sessions here get exactly such a `TMPDIR`. Issue 1925 saw the failure
while measuring something else and left it for this issue.

## Why it is a class, not a site

Flattening a path into a single file name (`tr '/' '_'`) has no bound by
construction. The marker's two properties are "unique per destination" and
"short enough to be a file name", and the flattening gives only the first.

Sweep (`git grep -nE "tr '/' '[_-]'|\\$\\{[a-z_]+//\\\\?/|replace\\(['\"]/['\"]|replace\\('/'|string\\(REPLACE \"/\"" -- scripts cmake just packages zephyr tests`),
every hit classified by what it flattens:

| Site | Flattens | Bounded? |
|---|---|---|
| `scripts/build/mirror-generated-header.sh` `mirror_ran_marker` | an ABSOLUTE build path | **no — this issue** |
| `scripts/build/compile-check-fixtures.sh` `_cc_target_dir` | a repo-relative fixture dir | yes (tracked path) |
| `scripts/build/fixtures-target-dir.sh` `nros_example_build_target_dir` | a repo-relative leaf dir | yes |
| `scripts/build/lint-example-leaf.sh` | a repo-relative example dir | yes |
| `packages/testing/nros-tests/tests/fixture_group_collision_gate.sh` | a repo-relative `Cargo.toml` path | yes |
| `just/zephyr-dev.just` `build-one-…` | an example name and a board name | yes |
| `scripts/build/zephyr-fixture-make-driver.sh` `target_name_for` | a make target name, not a file | n/a |
| `cmake/*` `string(MAKE_C_IDENTIFIER …)` | global property names, not files | n/a |
| Rust/Python `replace('/', …)` (board, topic, node, image names) | ROS / board identifiers | yes |

So one live site. Its only reader is `scripts/check-sizes-header-mirrors.py`,
which asks the script (`--marker`) rather than re-deriving the name, so the
fix has one place to land.

## What the marker is NOT

It is not a ninja output or byproduct: no `add_custom_command` names it, so
renaming it cannot strand a rule. A build dir carrying only an old-name
marker reads as "no marker" to the gate, which then takes issue 1901's
archive bridge — the state every pre-1911 dir was already in — and the next
build of that dir writes the new name. No absorbing state (issue 0834's
class) is created.

## Resolution

The marker is named by a new shared helper, `nros_path_key` in
`scripts/lib/path-key.sh`: the last 48 bytes of the path flattened with
`/` → `_` (so a listing still says which header a marker is for) plus the
first 16 hex digits of `sha256(<path>)` (so it stays unique per dest). At
most 65 bytes, whatever the build dir. `mirror_ran_marker` is its only
caller; the gate keeps asking `--marker`, so the name is still spelled once.

The mirror self-test (`mirror-header-precedence`, on the fast line) gains
two cases: a dest whose flattened path is past NAME_MAX records its run
under a name of at most 100 bytes, and two dests that share a long tail get
two markers. Mutation proof (each reverted, `cmp`-restored, green again):

| Mutation | Self-test |
|---|---|
| marker back to `tr '/' '_'` | FAIL — "a dest deeper than NAME_MAX records its run under a bounded name" |
| `nros_path_key` drops the hash (tail only) | FAIL — "two dests sharing a long tail get two markers" |
| `nros_path_key` keeps the whole flattened path | FAIL — "a dest deeper than NAME_MAX …" |

Symptom: the same long-`TMPDIR` copy-out run that failed above now reports
`cpp-port-minimal-publisher/mps2-an385-freertos: OK — 1 artifact(s)` and
`check-template-copy-out: OK` (the Zephyr leaf stays NOT VERIFIED, issue
1782, unchanged).

Every other flattening site in the sweep handles a repo-relative path or an
identifier and stays as it is; the table above is the classification.
