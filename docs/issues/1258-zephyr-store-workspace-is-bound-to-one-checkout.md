---
id: 1258
title: "A Zephyr workspace in the store is bound to the ONE checkout that
  provisioned it -- every later project builds that checkout's Zephyr module"
status: open
type: bug
area: zephyr, tooling
severity: medium
related: [issue-1254, issue-1259, phase-449]
---

## Symptom

`scripts/zephyr/setup.sh` ("Initialize Workspace") makes the workspace's west
manifest project a symlink to the checkout that ran it:

```
west init -l --mf "$MANIFEST" "$WORKSPACE_DIR/$NANO_ROS_NAME"
rm -rf "$WORKSPACE_DIR/$NANO_ROS_NAME"
ln -sf "$NANO_ROS_ROOT" "$WORKSPACE_DIR/$NANO_ROS_NAME"
```

with `.west/config` reading `[manifest] path = nano-ros`. West's project list
is what Zephyr's module discovery reads, so the `nros` Zephyr module of EVERY
build in that workspace comes from the symlink target. Measured on a
downstream (Autoware Safety Island) board build, `build/zephyr_modules.txt`:

```
"nros":"<workspace>/nano-ros":"/home/aeon/repos/simple-autoware-safety-island/third-party/nano-ros/zephyr"
```

That was harmless while a workspace sat beside ONE checkout. Since phase-440
W4 the default install target is `$NROS_STORE/workspaces/zephyr/<version>`,
and RFC-0095 D2's point is that "one Zephyr workspace must be shared by every
project that wants that version". The first project to provision a line now
binds every later project on the host to its nano-ros tree. A second project
pinning a different nano-ros builds the first one's `zephyr/` CMake and
Kconfig with its own `find_package(nano_ros)` runtime, and nothing reports the
mix. The two trees on this host already disagree:

| workspace | `nano-ros ->` |
| --- | --- |
| `~/repos/nano-ros/zephyr-workspace` (3.7) | `/home/aeon/repos/nano-ros` |
| `~/.nros/workspaces/zephyr/4.4` (provisioned by the island) | `.../simple-autoware-safety-island/third-party/nano-ros` |

It is also RFC-0095 D1 inverted: the provisioned tree does not live inside a
checkout, but it reaches INTO one, so deleting or moving that checkout breaks
every project's Zephyr build.

## Fix shape

The store workspace should carry no checkout. `setup.sh` already copies the
manifest file before replacing it with the symlink (west init follows
symlinks); keep the COPY as a manifest-only project, and let each build name
its own nano-ros Zephyr module, e.g. through `ZEPHYR_EXTRA_MODULES` from
`find_package(nano_ros)`, which every consumer already calls. Then the
workspace is keyed by Zephyr version alone, which is what its path claims.

## Also: provisioning re-fetches a line that is already on disk

A fresh `NROS_ZEPHYR_VERSION=4.4 just zephyr setup` into the store fetched
Zephyr at ~250 KB/s (165 MB in ~20 min, ~4.6 GB to go) on a host that already
held the same line at `~/repos/nano-ros-workspace-4.4`. West can seed from it:
`update.path-cache` makes `west update` clone each project from a local repo
with a plain `git clone` (`west/app/project.py`, "cloning from {cache_dir}";
no `--shared`/`--reference`), so the new tree carries no `objects/info/alternates`
and does not depend on the old one. Measured: the Zephyr repo arrived as an
891 MB pack in a few minutes, against 165 MB in twenty over the network. It is
a COPY, not hardlinks (link count 1, a repacked pack name), so it costs its
full size on disk. Passing it through a `WEST_CONFIG_GLOBAL` file reached
`setup.sh`'s plain `west update` unchanged.
`setup.sh` could accept a cache path, or offer the legacy checkout-relative
tree of the same line as one.

## Acceptance

- A store workspace's west project list contains no path into a checkout.
- Two projects pinning two nano-ros revisions build against one store
  workspace, each with its own `zephyr/` module (`zephyr_modules.txt`).
