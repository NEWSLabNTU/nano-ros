---
id: 1248
title: "The build system encoded distrobox; it should only ask whether ROS 2 ament is discoverable"
status: resolved
area: tooling, build
severity: medium
related: [0400, 0401, 0759, 0925, 1055, 1229, 1243, phase-440, RFC-0095]
---

# One question about an environment, not three

The build system carried a concept of *the box*: a second checkout, mirrored
from the host by rsync, with its own marker file, its own env variables, and a
push-lane gate whose whole subject was whether that mirror was faithful.

That is one environment shape out of many, encoded in a build system that has no
business knowing any of them. **The only question it may ask is whether ROS 2
ament packages are discoverable, so message packages can be found.** Whether the
Linux answering that question is bare metal, a container, a VM or a distrobox is
the operator's business.

## Why the mirror could never work

`ros2-box-sync.sh` excluded build output by directory NAME, and the tree has
tracked SOURCE with those names. The two cannot be told apart by name, because
**provisioned source and build output share directories** — the finding RFC-0095
records. So the rule set was a name-based approximation of a question git answers
exactly, and it was wrong five times:

| | what it ate | how it surfaced |
| --- | --- | --- |
| #1055 | `packages/cli/build-support/` | the box could not compile `nros` at all |
| #758 | Zephyr `drivers/i2c/target/` | could not configure any Zephyr image |
| #1229 | nested repositories | the sweep read one index while rsync copies the tree |
| #0925 | generated workspace manifests | (open at retirement) |
| #1243 | 2914 paths, four independent causes | qemu ×2, `external/`, esp-idf, pymavlink |

Every one is a bug in the COPY, not in nano-ros. `git clone` has none of them.

Each fix also anchored one pattern or added one `--include`, and the class
survived: the sixth instance was always going to be a directory somebody vendored
under a name that looks like output.

## What replaces it

Nothing. **Build where you run**: clone nano-ros inside the box and work there.
That was already the rule — CLAUDE.md carried "box in play => EVERY job in the
box, on its OWN tree" and issue 0759 refused sharing outright — so the mirror
existed only to make that rule convenient, and it reintroduced the hazard the
rule exists to prevent, one layer over.

## What survives, and why it is not a box concept

A distrobox **shares `$HOME` with the host by design**, so `~/.nros` and
`~/.cargo` are the same directory in both: two toolchains, two libcs, one store.
That is the general rule *a store belongs to one toolchain* meeting an
environment where a different machine does not imply a different `$HOME`.

`ros2-box-env.sh` therefore survives at 48 lines from 260, and sets exactly
`NROS_HOME` and `CARGO_INSTALL_ROOT` (plus the PATH that reaches the latter).
Gone with the mirror: the `CARGO_TARGET_DIR` redirect, `NROS_BOX_REPO`, the
`.nros-box-tree` marker and the `NROS_ALLOW_SHARED_BOX_TREE` escape. The redirect
was already documented IN THAT FILE as "actively harmful" in a box-owned tree —
the fixture contract is leaf-relative, so redirecting moves fixtures out from
under the tests that stat them — and it only ever served the mode 0759 refused.

`ros2-distrobox-setup.sh` stays: preparing an environment is a convenience for a
contributor, not a thing the build system reasons about.

## Removed

* `scripts/dev/ros2-box-sync.sh`
* `scripts/check-box-sync-covers-tracked-source.py` — **deleted, not relaned.**
  With no mirror there is nothing to be unfaithful to. This is the deliberate
  retirement `.config/gate-registry-baseline.txt` documents, so its name comes
  out of the baseline by hand, one line, rather than by regenerating the file
  (which would have swept in 27 unrelated gates added since it was last written
  and made the diff lie about what changed).

Also closes **#0925** as moot, and **phase-440 W2** — the lane question had three
candidates and the answer turned out to be that the gate should not exist.

## Verified

`check-gate-lists` 283 fast / 21 build-serial; `check-gate-visibility` 21
acknowledged; `check-doc-refs`; `just ci gate`.
