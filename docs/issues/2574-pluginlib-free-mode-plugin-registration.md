---
id: 2574
title: "`command_mode_decider` / `command_mode_switcher` load their plugins through pluginlib, which does not exist on an RTOS"
status: open
type: enhancement
area: [api, cpp]
severity: medium
found: 2026-11-13
related: [phase-560, rfc-0057]
---

GitHub: NEWSLabNTU/nano-ros#3762

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)**, with
the other L4 plan items (NR-15, NR-19, NR-12, NR-6). The fit is loose: this
is a component-registration question (RFC-0057's ament-parity registration),
not a contract one, and it moves if a registration phase opens. L4 plan item
NR-17, a prerequisite of the island's P0 core port.

## What is needed

The L4 island runs Autoware 2.0.0's `command_mode_decider` and
`command_mode_switcher` on Zephyr. Both load their mode and source plugins
through pluginlib, which does not exist on the RTOS.

## Direction

A static registration (a compile-time table or a registration macro) that the
ported nodes use in place of the plugin loader, keeping the same plugin
interfaces.
