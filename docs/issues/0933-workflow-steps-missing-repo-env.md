---
id: 933
title: "28 CI steps invoke `just`/`nros` without sourcing `./activate.sh`, and
  nothing gates the class"
status: open
type: bug
area: ci
related: [0639]
---

## What is wrong

CLAUDE.md's sweep contract says every `just <plat>` invocation needs
`source ./activate.sh` first, and `just doctor` enforces it *for a developer's
shell*. Nothing enforces it for a CI step, and CI steps are the one place where
the shell is fresh every time.

An audit of `.github/workflows/*.yml` — a step counts if its `run:` invokes
`just` or `nros` and its body contains neither `source ./activate.sh` nor
`source ./setup.bash`:

| workflow | steps missing the repo environment |
| --- | --- |
| `host-tests.yml` | 9 — **fixed**, see below |
| `nightly.yml` | 14 |
| `pr-checks.yml` | 11 |
| `post-submit.yml` | 2 |
| `queue-notify.yml` | 1 |
| `docs.yml`, `images.yml`, `nightly-report.yml`, `queue.yml` | 0 |

The `host-tests.yml` nine are fixed in the commit that files this issue. The
remaining 28 are untouched, deliberately — see "Why the rest is not fixed here".

## How it surfaced

`host-tests`' "Build workspace fixtures" step sourced `/opt/ros/humble/setup.bash`
and nothing else, so `nano_ros_ROOT` was unset and the generated workspace root
could not resolve its own package:

```
CMake Error at CMakeLists.txt:23 (find_package):
  By not providing "Findnano_ros.cmake" in CMAKE_MODULE_PATH this project has
  asked CMake to find a package configuration file provided by "nano_ros",
  but CMake did not find one.
```

`nano_rosConfig.cmake` lives at the **checkout root** and is located via
`nano_ros_ROOT`, which `activate.sh` exports (`activate.sh:83`). A generated
workspace root sets no prefix on purpose — its paths stay relative so they are
byte-identical across machines — so the environment is the only channel.

The failure was invisible locally in both directions at once, which is why it
took a cold reproduction to see:

- a developer shell has always sourced `activate.sh`, so the variable is set;
- a warm build directory carries `nano_ros_DIR:PATH=<checkout>` in its
  `CMakeCache.txt`, so even an unsourced re-configure succeeds on a tree that
  once worked.

Reproduce cold, and confirm the fix, with:

```bash
cd examples/workspaces/c
env -u CMAKE_PREFIX_PATH -u nano_ros_ROOT \
  cmake -S build/posix-zenoh-native -B /tmp/cold -DNROS_RMW=zenoh   # the error above
( source ./activate.sh && cmake -S build/posix-zenoh-native -B /tmp/warm -DNROS_RMW=zenoh )
                                                                    # -- Configuring done
```

`activate.sh` is a strict superset of the bare ROS line: it sources ROS itself
(picking the file the current shell can read, and nounset-guarded — issue 0639)
and additionally exports `NROS_REPO_DIR`, `nano_ros_ROOT`, the cargo `--locked`
PATH shim, and the toolchain directories. It only ever *prepends* to `PATH`, so
`$GITHUB_PATH` entries an earlier step added survive it.

## Why the rest is not fixed here

Adding `source ./activate.sh` is not behaviour-neutral. `activate.sh` puts
`scripts/bin/cargo` on `PATH`, which injects `--locked` project-wide
(`NROS_CARGO_FLAGS`, issues 0359/0378). Every one of the 11 `pr-checks.yml`
steps sits under the **required** `CI` aggregator check, so applying the sweep
blind would risk the merge queue on a change nobody had run. The 14 nightly
steps cannot be verified from a developer host at all.

That is a reason to sequence the sweep, not to skip it.

## What would close this

1. Apply `source ./activate.sh` to the remaining 28 steps, one workflow per
   pull request, each one observed green before the next — `post-submit.yml`
   and `queue-notify.yml` (3 steps) first, `nightly.yml` next, `pr-checks.yml`
   last.
2. Add a gate for the class. It is a pure source-level check over
   `.github/workflows/*.yml` — parse the YAML, and for each step whose `run:`
   invokes `just` or `nros`, require the body to source the activation script —
   so it belongs on the fast, buildless line. `check-just-recipe-refs` reads
   `just/*.just` and has never read `.github/workflows/`, which is the coverage
   gap that let three different spellings of "activate" coexist in one file.
3. Where a step legitimately needs no environment, it should say so in a
   comment and the gate should key on that marker rather than on absence.
