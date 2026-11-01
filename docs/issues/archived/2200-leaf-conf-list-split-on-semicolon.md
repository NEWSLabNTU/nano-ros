---
id: 2200
title: "A board `prj.conf` plus an image `os_config` fragment: the Zephyr hook split `NROS_LEAF_CONF` at its `;` and dropped the leaf's fragment, silently"
status: resolved
type: bug
area: [zephyr, cmake]
severity: high
found: 2026-11-01
resolved: 2026-11-01
related: [1782, 2157, 2179]
---

## What

`nros ws leaf-system` prints the image's Zephyr conf fragments as ONE row,
`;`-joined: the board package's files first (issue 1782), then the image's
`os_config`:

    NROS_LEAF_CONF=<board>/prj.conf;<leaf>/prj-serial.conf

The module hook (`zephyr/cmake/nros_image_kconfig.cmake`) read the CLI's stdout
with

    string(REPLACE "\n" ";" _lines "${_out}")
    foreach(_l IN LISTS _lines)

which turns the output into a cmake LIST, and the `;` inside the value is a
list separator too. The row became `NROS_LEAF_CONF=<board>/prj.conf`, the
leaf's fragment became a bare element that matched no row, and the loop skipped
it. The configure succeeded.

Found 2026-11-01 building a Zephyr 4.4 image for an out-of-tree board
(`xiao-rp2350`, whose package carries a `prj.conf`) with
`os_config = ["prj-serial.conf"]`: the uf2 linked with NO zenoh link —
`# CONFIG_NROS_ZENOH_LINK_SERIAL is not set`, no TCP either — and nothing said
so. The configure log showed it, for anyone who knew to look:

    -- nano-ros: image `xiao_serial` conf -> .../nros-board-xiao-rp2350/prj.conf (ahead of ...)

The workaround was to repeat the leaf's confs in `-DEXTRA_CONF_FILE`.

**The in-tree pair has the same shape and was hit the same way.**
`mps2-an385-zephyr` carries a `prj.conf`, and
`examples/templates/cpp-port-minimal-publisher/zephyr` names
`os_config = ["prj-zenoh.conf"]` (POSIX API, pthread counts, workqueue stack).
Its fixture row passes only `conf_files = ["prj.conf"]`, so since issue 1782
that fragment reached no build of it.

## Why it stayed invisible

Two halves, both needed:

1. A list is not a sequence of lines. Besides `;` splitting a line, an
   UNBALANCED `[` merges lines: cmake list splitting honours square-bracket
   nesting, so every separator up to the matching `]` is not one. Escaping
   `;` as `\;` (the spelling `NanoRosRmwDispatch.cmake` carried for its
   capability lists) fixes the first and not the second; there is no escape
   for a bracket.
2. Every reader SKIPPED a line it could not attribute. A skipped line is
   exactly what a split value looks like, so the reader turned a corrupt
   answer into a short one.

## Fix

- `cmake/NanoRosCliRows.cmake` — `nros_cli_rows(<prefix> <text> [REQUIRE
  <regex>] [CONTEXT <what>])`, the one splitter. It walks the newlines with
  `string(FIND)` and returns each line in its own variable, so no list is ever
  built from the text: `;`, `[`, `]` and `\` arrive intact. `REQUIRE` makes a
  line outside the producer's grammar a FATAL_ERROR naming it, instead of a
  skip.
- Both readers of `nros ws leaf-system` use it with
  `REQUIRE "^NROS_LEAF_[A-Z0-9_]+="` — the hook (its row loop is now
  `_nros_image_kconfig_leaf_rows`, so a gate can call it) and
  `nano_ros_read_leaf_system` (`cmake/NanoRosPackageXml.cmake`), which split
  `NROS_LEAF_CONF` / `NROS_LEAF_SNIPPET` the same way (it does not consume
  them, so nothing reached a build there).
- The hook refuses an `NROS_LEAF_CONF` element that is not an absolute path to
  an existing file: the CLI checked each one, so anything else is a mangled
  list.
- The CLI refuses to print a `;`-joined row (`NROS_LEAF_CONF`,
  `NROS_LEAF_SNIPPET`) whose item holds a `;`, a line break or an unbalanced
  bracket (`cmake_list_row`), since no reader could take that value apart.

**The class, swept.** `git grep -nE 'REPLACE +"\\n" +";"' -- '*.cmake'
'*CMakeLists.txt' ':!third-party'` found twelve sites. Eleven read `nros` CLI
output and now use `nros_cli_rows`: `NanoRosBoardFacts`, `NanoRosLocate`,
`NanoRosPackageXml`, `NanoRosProviders`, `NanoRosRmwDispatch`,
`NanoRosRmwUserConfig`, `NanoRosSizingDescriptor` (×2), `NanoRosWorkspace`,
`nros_cargo_build`, `nros_image_kconfig`. Only the two leaf-system readers had
a value that could carry a `;` today; the rest read paths and scalars, where it
would take a `;` or `[` in a path. `NanoRosBoardFacts` hands its rows on AS a
list (env entries for `corrosion_set_env_vars`), so it now refuses a value
holding a `;` or an unbalanced bracket rather than splitting it into two env
entries. The twelfth, `cmake/bootstrap.cmake`, reads `git submodule status`
and only tests for a leading `-`; it is left, and is the gate's one named
exception.

## Verified

- Real configure, Zephyr 4.4, `west build --cmake-only -b
  xiao_rp2350/rp2350a/m33 apps/talker-c -DNROS_IMAGE=xiao_serial` with this
  checkout as the module: `conf -> .../nros-board-xiao-rp2350/prj.conf;
  .../apps/talker-c/prj-serial.conf`, and `.config` has
  `CONFIG_NROS_ZENOH_LINK_SERIAL=y` (before: board conf only, link not set).
- Same, `-b mps2/an385` of `cpp-port-minimal-publisher/zephyr`: both files
  inserted, `CONFIG_POSIX_API=y`, `CONFIG_MAX_PTHREAD_MUTEX_COUNT=64` from
  `prj-zenoh.conf`.

## Gate

`just check cmake-cli-rows` (fast line, buildless `cmake -P`,
`packages/testing/nros-tests/tests/cmake_cli_rows.sh`): the splitter on `;`,
`[`, `]`, `\` and CRLF; `REQUIRE` refusing a stray line; the hook's reader fed
the in-tree mps2-an385-zephyr + cpp-port-minimal-publisher answer keeping both
files in order, refusing the pre-fix split shape, and refusing a non-file
element; and the sweep above. Mutation-checked: putting the old list split back
in `_nros_image_kconfig_leaf_rows` turns T3, T4, T5 and T6 red.
`the_in_tree_board_plus_leaf_conf_row_has_two_items` (nros-cli-core) pins that
the in-tree pair still produces a two-item row, so the gate keeps exercising
the shape.

## Not covered

A conf path or snippet name with a balanced `[...]` that also contains a `;`
is refused by the CLI, not carried. Paths with brackets are otherwise carried,
but the hook's later list operations on `_image_conf` (`IN_LIST`, `foreach`)
are ordinary cmake list operations; an unbalanced bracket never gets there.
