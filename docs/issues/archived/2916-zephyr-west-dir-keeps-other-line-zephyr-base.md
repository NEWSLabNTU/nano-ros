---
id: 2916
title: "A Zephyr build directory configured on one Zephyr line keeps that line's tree when the other line builds into it"
status: resolved
type: bug
area: [zephyr, build, ux]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [issue-2192, issue-2906, issue-2921]
---

## What happens

Host 142 found it, and host 241's FVP cell on 3.7 hit it. `nros build`'s west
build directory, `<ws>/build/<coordinate>/<bringup>__<image>_west`, carries no
Zephyr line in its name. After a 4.4 build, a build with
`NROS_ZEPHYR_VERSION=3.7` went into the same directory. Zephyr re-read the
cached `ZEPHYR_BASE` (the 4.4 tree) over the environment's 3.7 one, so the
configure ran 4.4's checks, asked for Python 3.12, and failed against a tree
nobody had chosen.

The fixture lanes avoid this by default, because their build root is keyed by
line (`zephyr-workspace-builds/<line>`, issue 2192). An explicit
`NROS_ZEPHYR_BUILD_ROOT` drops that key, though, and host 241 sets one. So the
fixture lanes collide there too. Reproduced on 2026-11-24 with
`build-ws-rust-talker-clock-sntp-zenoh`: configured on 4.4, then built on 3.7
in the same directory.

## Why not a per-line directory name

The directory name is documented to users (`book/src/reference/build-outputs.md`,
`book/src/guide/platforms/zephyr-native-sim.md`), and `nros run` / `nros flash`
and the printed `west build -d …` lines all name it. Putting the line in the
name changes a path users' scripts already hold, for every build, to fix the
case of switching lines. The case is real but rare.

Zephyr's own `west build -p auto` does not help: it compares the application
source directory, not the Zephyr tree.

## Resolution

Fixed 2026-11-24. Before running west, `nros build` reads `ZEPHYR_BASE:PATH=`
from the build directory's `CMakeCache.txt` and compares it, by canonical
path, with the tree this build resolved. When they differ it refuses, naming
the directory, both trees, and the one command that rebuilds the directory for
this tree:

    nros build <image> -- -p always

or `-- -d <dir>` to keep both builds. A caller that already passes a
rebuilding pristine flag (`-p`, `-p always`, `--pristine[=always]`) is not
refused; `-p auto` and `-p never` are. `--dry-run` never refuses. The check
covers nros's own directory and a `-d` the caller names, so the fixture lanes
are refused too, instead of configuring against the wrong tree.

Measured on host 241: the 3.7 build after the 4.4 one stopped before west
with

    build dir:      /tmp/nros-build-aeon/zephyr/build-ws-rust-talker-clock-sntp-zenoh
    configured for: /home/aeon/.nros/workspaces/zephyr/4.4/zephyr
    this build:     /home/aeon/.nros/workspaces/zephyr/3.7/zephyr

Unit tests cover the cache read (no cache, no entry, another tree, the same
tree spelled differently) and the pristine spellings.

## Left open

On a host with `NROS_ZEPHYR_BUILD_ROOT` set, the fixture lanes of the two lines
still share directories. They are now refused instead of mixing trees, but
alternating lines means rebuilding. Whether the explicit root should keep the
line as a sub-key is a separate decision about what that knob means.
