---
id: 2068
title: "`nros setup` for a NuttX board does not ask for the host tools its kernel build needs"
status: open
type: bug
area: [nuttx, setup, installed-sdk]
severity: medium
found: 2026-10-26
related: [phase-517, issue-2064, RFC-0062]
---

## What

`nros setup qemu-armv7a-nuttx` (and `rv-virt-nuttx`) provisions the cross
toolchain, QEMU and the NuttX kernel/apps/libc sources, but not two host tools
the NuttX kernel build then needs:

- Python's `venv` module — `scripts/nuttx/build-nuttx.sh` provisions
  `kconfiglib` into a venv when `kconfig-frontends` is absent;
- `unzip` — NuttX's own build downloads and unzips `dtc` and `libmetal`.

The index already declares both (`[prereq.python3-venv]`, `[prereq.unzip]`,
and `[prereq.kconfig-frontends]` as the alternative), but nothing ties them to
a NuttX board: a `[tool.*]` can say `system = [..]` and the setup plan asks for
those keys before the download (that is how `[tool.zephyr-sdk]` asks for
`wget`), while a `[source.*]` (`SourcePackage`) and a `[board.*]`
(`BoardEntry`) have no such field.

Found replaying `integration-nuttx.md` on the installed track (phase-517 W10).
The chapter now names both as a step, which is the workaround, not the fix.

## Fix direction

Give `[source.*]` (or `[board.*]`) the same `system = [..]` list a tool has,
read by the same plan code, and declare `python3-venv` + `unzip` on
`nuttx-kernel`. Then drop the chapter's manual step.

## Acceptance

On a host without `python3-venv` and `unzip`, `nros setup qemu-armv7a-nuttx
--dry-run` lists both among the system packages it would install.
