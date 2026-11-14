---
id: 2569
title: "The C++ metadata probe constructed a `configure`-shape class with the `NodeHandle` constructor, and derived a nested namespace's header differently from the entry"
status: resolved
type: bug
area: [cmake, cli]
severity: low
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "issue 2339's fix (41679cafb3) for the shape; the change that resolved this one for the header"
related: [phase-478, issue-2339]
---

GitHub: NEWSLabNTU/nano-ros#3861

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

The C++ metadata probe assumes a node's header is `<pkg>/<Class>.hpp`. A
ported Autoware package with a different header path fails the probe. Setting
`HEADER` (on `nano_ros_node_register`) works around it, but the probe still
logs an rclcpp-shape failure.

## Direction

With `HEADER` set the probe uses it and logs nothing; without it, the error
names the `HEADER` option.

## What the repro showed

NXP 175's repro, on 0.6.0-nros8, corrects the report: `HEADER` was honoured.
The call was:

```cmake
nano_ros_add_node(si_placeholder_cpp
  CLASS autoware_si_placeholder_cpp::Placeholder
  HEADER autoware/si_placeholder_cpp/placeholder.hpp
  SOURCES src/placeholder.cpp)
```

The class is the `configure` shape: a default constructor plus
`configure(rclcpp::Node &)`. The probe constructed it with the handle anyway:

```
error: no matching function for call to 'autoware_si_placeholder_cpp::Placeholder::Placeholder(rclcpp::NodeHandle&)'
```

The image entry built and ran, because cmake took `nano_ros_add_node`'s own
default shape, `configure`.

## Resolution

**The shape.** Issue 2339's fix, 41679cafb3 (2026-11-09), already makes the
probe take a component's shape from the verb that declared it. The scanner
reads `SHAPE`, and when there is none it takes the verb's cmake default,
through `stated_shape`. That is the same decision the entry is built from.
It landed after 0.6.0-nros8 was cut (2026-11-07), which is why the island
still saw the failure. 0.6.0-nros9 carries it. A stale failure recorded by
the old CLI is not reused, because the probe's negative cache is keyed on
the CLI's planning stamp.

**The header.** One real divergence remained. With no `HEADER`, the probe
derived `<outermost namespace>/<Class>.hpp`. `nano_ros_node_register` derives
the header the entry includes by turning every `::` into `/`. So
`autoware::foo::Bar` probed `autoware/Bar.hpp` and built against
`autoware/foo/Bar.hpp`. `ComponentDeclaration::probe_header` now derives it
as cmake does.

Tests:
- `the_probe_constructs_each_shape_as_the_entry_does`: the `configure` shape
  is never given the handle constructor, and the `rclcpp` shape is never
  configured.
- `a_ported_add_node_call_is_probed_with_its_header_and_the_configure_shape`:
  the island's call verbatim.
- `nested_namespaces_map_to_nested_directories_as_the_entry_does`.
