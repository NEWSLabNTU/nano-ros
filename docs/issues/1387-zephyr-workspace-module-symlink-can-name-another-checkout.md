---
id: 1387
title: "a Zephyr west workspace inside one checkout can have its `nano-ros`
  module symlink pointing at ANOTHER checkout, so every image built there is
  half of each and nothing says so"
status: open
type: bug
area: [zephyr, tooling]
severity: high
found: 2026-09-18
related: [issue-1280, issue-0759, phase-455]
---

## What happened, measured

`/mnt/wd/data/projects/nano-ros-box` is a second checkout, used because ROS 2
lives in a distrobox and the host has a different libc and compiler. It carries
its own `zephyr-workspace/`. That workspace's west manifest module link was:

```
$ ls -l zephyr-workspace/nano-ros
zephyr-workspace/nano-ros -> /mnt/wd/data/projects/nano-ros      # the OTHER checkout
```

So `west build` inside the box checkout compiled:

* the **application** from this checkout —
  `APPLICATION_SOURCE_DIR:PATH=/mnt/wd/data/projects/nano-ros-box/examples/zephyr/rust/action-server`
* the **nano-ros Zephyr module** from the other one — the `Includes:` line of
  the configure names
  `/mnt/wd/data/projects/nano-ros/packages/platform/nros-platform-api/include`,
  `/mnt/wd/data/projects/nano-ros/packages/rmw/zenoh/zpico-sys/c/include` and
  `.../zpico-zephyr/include`, and `_NROS_RMW_DISPATCH_DIR`,
  `_NROS_MESSAGE_BOUNDS_DIR` and `_NROS_KNOB_INVENTORY_FILE` in `CMakeCache.txt`
  all resolve there.

One image, two trees, on two different branches. The Rust half was this
checkout's `nros-rmw-zenoh`; the C shim it calls was the other checkout's
`zpico.c`. Nothing warned. The symlink is dated 2026-08-15, so every Zephyr
image built in that workspace since has been mixed.

It was found only because a build log was read line by line for an unrelated
reason (phase-455 W4). The fix in the moment is one command:

```
ln -sfn "$PWD" zephyr-workspace/nano-ros      # then a pristine build
```

and a pristine build is required, not a reconfigure: the module root is a
configure-time identity, and the cached `_NROS_*_DIR` entries are what a
reconfigure would be asked to correct. After it,
`grep -c 'projects/nano-ros/packages' build.ninja` is 0 and
`projects/nano-ros-box/packages` is 760.

## Why this is not just operator error

It is a silent, durable, cross-checkout artifact mix, which is the class the
repo already refuses elsewhere: issue 0759 made the box refuse to work in the
host's tree, and issue 1280 made every path-valued variable re-root onto the
checkout being built when it names a DIFFERENT checkout. That rule — *outside
any checkout, keep; a different checkout, re-root; this one, keep* — is exactly
the rule this symlink breaks, and the west workspace is the one place it is not
applied, because the link is data in a gitignored directory rather than an
environment variable.

The dangerous property is the one the box memory already records: the failure
is not the loud `GLIBC_2.39 not found`. A mixed tree BUILDS, links and runs, and
the two halves only disagree when one of them changes.

## A second, smaller crossing in the same workspace

The same provisioning left the box checkout's own venv console scripts pointing
at the other checkout:

```
$ head -1 scripts/zephyr/.venv/bin/west
#!/mnt/wd/data/projects/nano-ros/scripts/zephyr/.venv/bin/python3
```

so `west` — and therefore `WEST_PYTHON` and `_Python3_EXECUTABLE` in every
Zephyr `CMakeCache.txt` — runs under the other checkout's interpreter. That one
is benign today (a build driver, not a compiler) and is left as measured rather
than fixed, but it is the same crossing and it survives the symlink repair.

## What would close it

1. A check that a resolved Zephyr workspace's `nano-ros` module link resolves
   to the checkout the build is being driven FROM, failing with the `ln -sfn`
   remedy. The marker walk `scripts/lib/checkout-paths.sh` already owns for
   issue 1280 answers "which checkout" — this is that predicate applied to one
   more path, not a new mechanism.
2. `just zephyr setup` writing the link from the resolved repo root rather than
   from `$PWD`-at-the-time, so the state cannot be created in the first place.
3. `just doctor` reporting it, since the existing workspaces on this disk are
   already in the bad state and nobody will re-run setup.

## What is NOT claimed

Which results this invalidated. Every Zephyr image built in that workspace
between 2026-08-15 and 2026-09-18 was mixed, and whether any of them was WRONG
depends on how far the two checkouts had diverged at each moment. The one
measurement taken here (phase-455 W4's goal-completion probe) was re-run from
scratch after the repair and reports the same numbers, which says nothing about
the other leaves.
