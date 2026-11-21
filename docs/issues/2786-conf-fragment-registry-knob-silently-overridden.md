---
id: 2786
title: "A Zephyr conf fragment that sets CONFIG_NROS_CAPABILITY_ROSOUT=n is silently overridden to y; it should be refused, naming the system.toml spelling"
status: open
type: bug
severity: medium
area: [zephyr, cli, build]
related: [phase-556, issue-2785]
found: 2026-11-21
---

**Home: [phase-556](../roadmap/phase-556-ros2-compat-audit.md) W13.**

## What happens

A consumer tried to turn off only rosout on a Zephyr image (issue 2785) with
an os_config fragment holding `CONFIG_NROS_CAPABILITY_ROSOUT=n`. The build
succeeded and the built `.config` still had `CONFIG_NROS_CAPABILITY_ROSOUT=y`:
the node-interface default, rendered later in `EXTRA_CONF_FILE`, won, and
nothing said so.

The rest of the tree refuses a conf fragment that sets a registry-owned knob
and names its `system.toml` spelling. The capability symbols escape that
check, so a user's explicit setting is dropped with no error.

## Wanted

A conf fragment that sets a `CONFIG_NROS_CAPABILITY_*` symbol owned by the
node-interface selection is refused at configure, naming the `system.toml`
key that owns it (`node_interfaces`, or a per-interface form if one exists),
the same way other registry-owned knobs are refused.

## Acceptance

A configure test: a fragment setting `CONFIG_NROS_CAPABILITY_ROSOUT=n` is
refused with a message naming the owning key; with no such fragment the
configure is unchanged.
