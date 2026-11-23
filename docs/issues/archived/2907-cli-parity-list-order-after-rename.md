---
id: 2907
title: "The ROS 2 CLI parity harness compared `ros2 service list -t` in printed order, after renaming each side's node: a correct answer failed whenever the two node names sorted differently against a service name"
status: resolved
type: bug
area: [testing, interop]
severity: medium
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "nros-tests ros2_compat: a flat `<noun> list` answer is compared sorted after normalising, and the diff escapes hidden characters and names a reordering"
related: []
---

## What failed

The 2026-11-21 live-peer regression job (run 44864729982) failed
`ros2_cli_parity_interop::service_list_with_types_answers_as_stock_does`:

```text
+ /add_two_ints [example_interfaces/srv/AddTwoInts]
  <node>/describe_parameters [rcl_interfaces/srv/DescribeParameters]
  ... the other five parameter services ...
- /add_two_ints [example_interfaces/srv/AddTwoInts]
  [exit 0]
```

The `-` and `+` lines are byte-identical. Both raw answers in the junit are
right: nano-ros's node is `/add_server`, stock's is `/add_two_ints_server`, and
`ros2 service list` prints its list sorted by the raw name. `/add_server/...`
sorts before `/add_two_ints`, and `/add_two_ints` sorts before
`/add_two_ints_server/...`, so the service is last on one side and first on the
other.

## Cause

`ros2_compat::compare` renames each side's node to one placeholder (`<node>`)
and then compares the lines in printed order. The rename keeps each line's text
but not its sort position, which came from the old name. So for any list
query, the order compared was a property of the two node names, not of the
answers. The test could only pass when both node names sorted the same way
against every other name in the list.

The diff printer made it look like a hidden-character difference, since it
showed the same text as both removed and added and said nothing about order.

## Fix

- `compare` sorts both normalised answers again for a flat `<noun> list` query
  (`service`, `topic`, `action`, `node`). `param list` nests names under each
  node, so it is still compared as printed.
- `line_diff` escapes characters a terminal hides (control characters, non-space
  whitespace, zero-width characters, a BOM). When a line is marked both `-` and
  `+`, it says the answers differ in order there.

The other list comparisons in the suite were not affected. `topic list` and
`action list` lines name no node, and the parameter-service comparison filters
to `<node>/` lines, which share one prefix. They now go through the same sort.

Unit tests in `ros2_compat` use run 44864729982's two answers as fixtures.
