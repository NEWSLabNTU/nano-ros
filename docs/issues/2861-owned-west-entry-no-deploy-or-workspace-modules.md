---
id: 2861
title: "An owned Zephyr west entry gets no deploy and none of the workspace's Zephyr modules; a generated app gets both"
status: open
type: bug
severity: medium
area: [cli, zephyr, cmake]
related: [issue-2594, issue-2436, issue-2433, rfc-0071]
found: 2026-11-23
---
Found by a downstream safety-island port (its Zephyr FVP test images); seen
on main 545ae12ec1.

## What

`nros build <bringup>:<image>` for an image whose `entry =` names a
hand-written Zephyr west application (an owned entry, RFC-0071 D19) passes the
west configure two things fewer than a generated application gets:

1. **No deploy or board.** The entry's `nano_ros_add_executable(...)` names no
   `DEPLOY`/`BOARD` (it is the image's program, and the image already says
   where it runs), so the configure stops:
   - with sources: `nano_ros_entry: DEPLOY required` (the Zephyr arm of
     `nano_rosConfig.cmake` sets `NROS_DEPLOY` from the leaf `system.toml`
     only, and an owned entry has none);
   - with `DEPLOY ${NROS_IMAGE}`: `DEPLOY target '<image>' rejected --
     embedded Entry pkgs need a Board`;
   - LAUNCH-only: `codegen entry-pack --lang c --board ` (empty board key).
   A generated app writes `BOARD zephyr ... DEPLOY zephyr`
   (`builder/west_app.rs`, `render_cmakelists`).
2. **No workspace Zephyr modules.** The west line carries
   `-DZEPHYR_EXTRA_MODULES=<nano-ros>` only (`builder/zephyr.rs` `module_arg`),
   so a dependency of the entry that is a Zephyr module (`zephyr/module.yml`)
   never joins and its Kconfig symbols are undefined. A generated app gets the
   modules of its package closure (issue 2594, phase-478 I10/I11).

The port works around both in the entry's own CMakeLists: it states
`BOARD zephyr DEPLOY ${NROS_IMAGE}` and appends its support package to the
cached `ZEPHYR_EXTRA_MODULES`.

## Repro

The issue-2405 fixture `packages/testing/nros-tests/fixtures/west_hand_written_c_entry`,
copied out, with two edits:

- `src/zephyr_entry/CMakeLists.txt`: drop `BOARD zephyr` and `DEPLOY zephyr`;
- add `src/support_pkg` (`package.xml`, `zephyr/module.yml` with
  `build: kconfig: zephyr/Kconfig`, a Kconfig defining `config DEMO_SUPPORT`),
  `<depend>support_pkg</depend>` in `zephyr_entry/package.xml`, and
  `CONFIG_DEMO_SUPPORT=y` in `zephyr_entry/prj.conf`.

`nros build demo_bringup:zephyr` (native_sim, Zephyr 3.7):

```
prj.conf:9: warning: attempt to assign the value 'y' to the undefined symbol DEMO_SUPPORT
error: Aborting due to Kconfig warnings
```

with the west line holding `-DZEPHYR_EXTRA_MODULES=<nano-ros>` alone. Without
the Kconfig line the configure reaches the entry and stops on the empty board
key above.
