---
id: 1550
title: "On the nano_ros_add_executable road nothing compared the image's domain
  with system.toml or the snippet, and the boot record could not say which
  domain the image ran on"
status: resolved
type: bug
area: zephyr, cmake
severity: medium
found: 2026-09-28
resolved_in: "feat/zephyr-entry-domain-agreement (this PR)"
related: [issue-1423, rfc-0049, phase-460]
---

## What happened

A Zephyr image's session takes `CONFIG_NROS_DOMAIN_ID` (RFC-0049: Kconfig is
the single writer). Issue 1423 added a refusal when that disagrees with the
bringup's `system.toml`, but only inside `nros_system_generate`. Images built
with `nano_ros_add_executable` (the Autoware Safety Island's three entries)
never reach it: the entry's `system.toml` is read by
`nano_ros_read_leaf_system`, its `domain_id` is PRINTED
(`deployment from .../system.toml ... domain_id=10`) and compared with nothing.

The transport snippet is the third witness and the one that bit: the island's
`island-serial`, `qemu-serial` and `qemu-ethernet` snippets state
`CONFIG_NROS_DOMAIN_ID=10`, and `island-ethernet` did not, so the Ethernet
board image would have joined domain 0 while every document said 10. A peer
on another domain never sees the image and nothing at run time says why; the
only record of the domain on a board with no console was the island's own
trace provenance, which carried the Cyclone domain only.

## Resolution

- `cmake/NanoRosDomainAgreement.cmake` reads Zephyr's `merge_config_files`
  (the fragments merged into `.config`, in order) and classifies each one that
  states the symbol: a snippet (beside a `snippet.yml`), the command line
  (`extra_kconfig_options.conf`), or any other `.conf`. The last one wins, as
  in Kconfig; none is the default, 0.
- `nros_check_domain_agreement` REFUSES when `CONFIG_NROS_DOMAIN_ID`, the
  entry's `system.toml` `domain_id` (when present) and every fragment that
  states the symbol do not agree, naming the three values:

      nano_ros_entry(qemu_entry): this image's ROS domain is stated more than once and the statements disagree:
        CONFIG_NROS_DOMAIN_ID = 0  (what the image bakes; from the Kconfig default -- no merged fragment states it)
        system.toml domain_id = 10  (.../src/qemu_entry/system.toml)
        snippet               = (not stated; active snippets: nros-zenoh, qemu-ethernet)

  It runs from `nano_ros_entry`'s Zephyr branch (what
  `nano_ros_add_executable` calls) and, for the snippet half, from
  `nros_system_check_domain_agreement` too, so both roads refuse the same
  disagreements. It changes no precedence.
- The boot record (version 7) appends `domain_id` = the id the boot-config
  resolver settled on | its `KnobSource` << 8. The source of a baked domain is
  the fragment the configure found (`NROS_KNOB_SOURCE_NROS_DOMAIN_ID`, one of
  default / kconfig / snippet / command-line, forwarded to cargo);
  `ROS_DOMAIN_ID` on a hosted image reads as `Environment`.
  `read-boot-report.py` prints it under "resolved at boot".
- Gate: `tests/cmake-entry-domain-agreement-tests.sh`
  (`just check entry-domain-agreement`).

## Not done here

`nros doctor` does not read an image's `.config`; the island's `just
board-doctor` prints the domain its image, each snippet, `system.toml` and the
shell's `ROS_DOMAIN_ID` resolve to, because the snippet set is the
application's, not nano-ros's.
