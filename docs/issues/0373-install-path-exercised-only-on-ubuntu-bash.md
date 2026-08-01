---
id: 373
title: "The book's install path is exercised only on ubuntu+bash — on Arch Linux three of its steps are wrong or unactionable"
status: open
type: tech-debt
area: build
related: [rfc-0014, rfc-0062, issue-0204, issue-0368, issue-0372, phase-327]
---

# The book's install path is exercised only on ubuntu+bash

## Summary

`book/src/getting-started/installation.md` is the front door for users, and its
only executable coverage is `just probe bootstrap` (issue 0204), which runs
**`ubuntu:24.04` under `bash`**:

```
scripts/probe/run-bootstrap-probe.sh:33   PROBE_IMAGE="${PROBE_IMAGE:-ubuntu:24.04}"
scripts/probe/run-bootstrap-probe.sh:87   && bash /probe.sh
```

`PROBE_IMAGE` is overridable but nothing ever overrides it, and no lane runs a
non-bash shell. Walking the same page on Arch Linux surfaced three defects, none
of which the probe can see. (The zsh activation abort found on the same run is
issue 0372; the zenohd source-build surprise is issue 0374.)

## Findings

**F1 — the prereq block is apt-only, with no per-distro mapping.**
The page's single `probe=10` block is

```sh
sudo apt-get install -y git curl ca-certificates build-essential pkg-config python3
```

with no note that non-Debian hosts need a translation. On Arch the mapping is
`build-essential` → the `base-devel` **group** (`ca-certificates` is pulled in by
the base system; the rest keep their names). Nothing in the book, the index, or
`just doctor` says so. This is the user-path twin of issue 0368's contributor-path
prereq findings, and RFC-0062's abstract-OS-package-key model is the structural
fix for both.

**F2 — `just` is a de-facto prereq of the user path, contradicting the page.**
installation.md:133 states `just` is **NOT** a prereq. But `activate.sh:156`
unconditionally sources `scripts/sdk-env.sh`, which shells out to `just`:

```
$ source ./activate.sh
nano-ros sdk-env: just not found; SDK defaults not loaded
```

So a user following the documented four-step flow silently gets a shell with no
SDK defaults. Either the defaults are load-bearing (then `just` is a prereq and
the page is wrong), or they are not (then the warning is noise and `sdk-env.sh`
should carry a `just`-free fallback).

**F3 — the ROS 2 warning is unactionable on distros with no Humble packages.**

```
activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes will fail
```

Arch ships no ROS 2 Humble (AUR build or a container, both multi-hour or
multi-GB). The message names no remedy and, more importantly, the book never
scopes **which** user-path steps actually need ROS — `activate.sh:47-50` says
`nros generate-rust`, cyclonedds codegen and rmw_zenoh interop, but a first-time
reader on a ROS-less host cannot tell whether the "First Node — Rust" walkthrough
is among them. Related: issue 0368 F4 (the bundled interface set is too small for
the repo's own example workspaces, which is what makes ROS-less codegen fail in
practice).

## Direction

1. Run the existing probe against a second, non-Debian image in a periodic lane
   (`PROBE_IMAGE=archlinux just probe bootstrap`) and a non-bash shell — the
   mechanism already exists, only the coverage is missing.
2. Give the prereq block a per-distro table (or, per RFC-0062 / phase-327,
   generate it from abstract package keys in `nros-sdk-index.toml`).
3. Decide whether `sdk-env.sh`'s defaults are load-bearing and make the page and
   the code agree.
4. State in the book which steps require a ROS 2 install and which do not.
