---
id: 933
title: "28 CI steps invoked `just`/`nros` without sourcing `./activate.sh`, and
  nothing gated the class"
status: resolved
type: bug
area: ci
related: [0639]
---

## What was wrong

CLAUDE.md's sweep contract says every `just <plat>` invocation needs
`source ./activate.sh` first, and `just doctor` enforces it *for a developer's
shell*. Nothing enforced it for a CI step, and a CI step is the one place the
shell is fresh every time.

`activate.sh` exports `nano_ros_ROOT`, and that is how `find_package(nano_ros)`
resolves: `nano_rosConfig.cmake` lives at the checkout root, and a generated
workspace root sets no CMake prefix on purpose — its paths stay relative so they
are byte-identical across machines. Without the variable:

```
CMake Error at CMakeLists.txt:23 (find_package):
  ... asked CMake to find a package configuration file provided by "nano_ros",
  but CMake did not find one.
```

Invisible locally in both directions, which is why it needed a cold reproduction:
a developer shell has always sourced `activate.sh`, and a warm build directory
carries `nano_ros_DIR:PATH=<checkout>` in its `CMakeCache.txt`, so even an
unsourced re-configure succeeds on a tree that once worked.

## Where it landed

| workflow | steps | fixed by |
| --- | --- | --- |
| `host-tests.yml` | 9 | #92 — the workspace fixture build |
| `nightly.yml` | 17 | #105 — 13 zephyr steps (four Zephyr 3.7 cells) + 4 platform |
| `pr-checks.yml` | 11 | this change |
| `post-submit.yml` | 2 | this change |
| `queue-notify.yml` | 1 | **not a defect — see below** |
| `docs`, `images`, `nightly-report`, `queue` | 0 | — |

Sequenced one workflow per pull request rather than swept blind, because
sourcing `activate.sh` is not behaviour-neutral: it puts `scripts/bin/cargo` on
PATH, which injects `--locked` project-wide (issues 0359/0378). The eleven
`pr-checks` steps sit under the required `CI` aggregator, so they went last,
after the same commands had been observed green under that shim locally.

Where a step already sourced `/opt/ros/humble/setup.bash`, `activate.sh`
REPLACES it rather than joining it: it sources ROS itself, choosing the file the
current shell can read and guarding nounset (issue 0639), and additionally
exports `nano_ros_ROOT`. It only ever PREPENDS to PATH, so the `$GITHUB_PATH`
entries earlier steps add — the in-tree CLI, `play_launch_parser` — survive it.

## The gate, and the two false positives that shaped it

`check-workflow-repo-env` (fast line, buildless). `check-just-recipe-refs` reads
`just/*.just` and `.github/workflows/*.yml` but only for recipe NAMES; nothing
looked at whether the step could run at all.

A gate here is only worth having if it can tell a command from a sentence about
a command. Both of these were live in the tree:

* **Prose.** `nightly.yml`'s CLI-build step carries a comment about `nros sync`
  and does not run it. Counting words in comments is the mistake that makes a
  grep-based gate useless, and it is the mistake the sizes-header and
  vacuous-test gates were each written to avoid.

* **Command text inside a heredoc.** `queue-notify.yml` builds a pull-request
  comment with `body="$(cat <<MSG … MSG)"` whose text tells the author to run
  `just queue-triage` and `just ci l1`. Those are STRINGS posted to GitHub, not
  commands. "Fixing" that step would have sourced an environment nothing in it
  uses, and would have taught the next reader that the gate is noise.

So the extractor skips comment lines and heredoc bodies, and a heredoc does not
swallow the rest of the body — an invocation after the terminator still counts.
Ten self-test cases cover both false positives, both activation spellings, the
`KEY=value just …` prefix form, and the after-heredoc case. Mutation-checked:
reverting one fixed step makes the gate exit 1 and name that step.

## Verified

`just check fast` — 140 gates, 4 skipped for host preconditions. The gate reports
9 workflows, 146 steps, every invocation sourcing the repo environment.
