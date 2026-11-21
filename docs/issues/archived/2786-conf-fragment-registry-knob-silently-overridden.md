---
id: 2786
title: "A Zephyr conf fragment that sets CONFIG_NROS_CAPABILITY_ROSOUT=n is silently overridden to y; it should be refused, naming the system.toml spelling"
status: resolved
type: bug
severity: medium
area: [zephyr, cli, build]
related: [phase-556, issue-2785]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(zephyr): refuse a conf fragment that sets a node-interface capability"
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

## Resolution

The module's conf gate, `_nros_image_kconfig_refuse_conf_knobs`
(`zephyr/cmake/nros_image_kconfig.cmake`), refused only the registry knobs
(`NROS_KNOB_KCONFIG_IDS`), which have no Kconfig prompt. The node-interface
capability symbols have a prompt, so a fragment could assign them, and the
build then set them again after every conf file: `nros build` as a
command-line Kconfig value (`west_app::capability_kconfig_cmake`), a leaf in
its rendered fragment. The fragment's value lost without a word.

The gate now also refuses `CONFIG_NROS_CAPABILITY_PARAM_SERVICES`,
`CONFIG_NROS_CAPABILITY_ROSOUT` and `CONFIG_NROS_CAPABILITY_SIM_TIME` in any
conf file the image merges, naming the file, the line and
`node_interfaces` as the owning `system.toml` key (with the axis's
`features` entry). The list mirrors `node_interfaces::STOCK`.

No per-interface opt-out exists yet, so turning off rosout alone is still not
expressible; that is a separate feature, not this fix.

Check: `just check zephyr-conf-capability-refusal`
(`tests/zephyr-conf-capability-refusal-tests.sh`): a fragment setting each of
the three symbols is refused naming `node_interfaces`; a fragment with
Zephyr-native symbols only configures unchanged; a registry knob is still
refused with its spelling; the module's list equals `STOCK`.
