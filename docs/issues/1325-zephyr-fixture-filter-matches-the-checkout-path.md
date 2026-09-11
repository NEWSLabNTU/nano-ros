---
id: 1325
title: "`NROS_ZEPHYR_FIXTURE_FILTER` is matched against a haystack containing the
  ABSOLUTE checkout path, so a worktree named after an rmw selects all 54 leaves"
status: open
type: bug
area: [build, testing]
severity: low
found: 2026-09-11
related: [1010, phase-448]
---

## What

`scripts/build/zephyr-fixture-leaves.sh`:

```sh
filter_haystack="$board $build_name $src_rel $conf_files $id"
if [ -n "$fixture_filter" ] && ! [[ "$filter_haystack" =~ $fixture_filter ]]; then
    continue
fi
```

`$conf_files` carries ABSOLUTE paths (`prj.conf;prj-zenoh.conf;/…/<checkout>/zephyr/native-sim-nsos.conf`),
so the checkout's own directory name is part of every leaf's haystack.

## Measured

In a worktree at `/home/aeon/nros-agent-worktrees/448-xrce`:

```
$ scripts/build/zephyr-fixture-leaves.sh --emit records … --filter 'xrce' | wc -l
57          # every leaf: zenoh, xrce and cyclonedds
$ scripts/build/zephyr-fixture-leaves.sh --emit records … --filter '/xrce$' | wc -l
18          # the actual xrce leaves
```

`build-(rust|c|cpp)-.*-xrce` matches everything too, because `.*` in a bash
`=~` crosses the spaces between the haystack's fields, so a `build-…` prefix in
one field pairs with an `-xrce` in another.

## Cost

`just zephyr build-fixtures` with `NROS_ZEPHYR_FIXTURE_FILTER=xrce` starts
building all 54 native_sim leaves and reports nothing unusual — it prints the
filter it was given, and the leaf list it prints looks like the lane working
normally. Two 10-minute build windows went into zenoh leaves nobody asked for
before the target list was read carefully. The failure is silent in the
expensive direction; a filter that selected too FEW would have said "no Zephyr
fixtures matched".

## Fix direction

Match per FIELD rather than against a joined string — the caller almost always
means the build name, which is the one field with a stable vocabulary. Either
restrict the haystack to `$board $build_name $id`, or anchor each field
separately. Any fix should keep working when the checkout is at a path
containing `zenoh`, `xrce`, `cyclonedds`, `rust`, `c` or `cpp`; a selftest
running the filter from a temp dir whose name contains `xrce` is the negative
control.

Not a blocker for anything, and there is a working spelling today (`/xrce$`,
which anchors on `$id`'s trailing rmw) — but it is a footgun whose cost is paid
in build minutes, and the workaround has to be rediscovered each time.
