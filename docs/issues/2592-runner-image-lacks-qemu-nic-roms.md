---
id: 2592
title: "The self-hosted runner image has qemu-system-aarch64 without its NIC option ROMs, so the Zephyr a53 core-pin cell exits at QEMU startup"
status: open
type: bug
area: [ci, zephyr]
severity: medium
found: 2026-11-15
related: [issue-2113, issue-1457, issue-1482]
---

## What happens

The merge lane's zephyr shard runs on the self-hosted runner. Two dispatches of
`merge-runtime.yml` (runs 43643676703 and 43650687503, 2026-11-14/15) each ran
all 3638 tests, and each had exactly one real failure, the same one:

```
sched_dims: 1 of 6 cell(s) FAILED:
  CorePinPlacement/zephyr/c: [zephyr c CorePinPlacement] expected the ACCEPT marker
  (`nros: core pin observed sched_task=`high` running_on=1`); saw fallback? ...
log:
qemu-system-aarch64: failed to find romfile "efi-e1000.rom"
nros run: the target exited with code 1 after 0.0 s
```

The image never booted. QEMU refused to start.

## Why

The cell boots Zephyr `qemu_cortex_a53` with `-nic user,model=e1000`, and QEMU
loads that NIC's option ROM, `efi-e1000.rom`, when the machine is built. On
Debian/Ubuntu the ROM is shipped by `ipxe-qemu` (measured on jammy:
`dpkg -S /usr/lib/ipxe/qemu/efi-e1000.rom` → `ipxe-qemu`). `qemu-system-arm`
only lists `ipxe-qemu` under **Recommends**.

Every image `scripts/ci/lib/image-context.sh` generates installs with
`--no-install-recommends`, so the `[prereq.qemu-system-aarch64]` row
(`apt = ["qemu-system-arm"]`, added for issue 2113) brought in the emulator and
not the ROM. A workstation `apt install qemu-system-arm` pulls recommends, which
is why the cell passed when run by hand. The row's `check = { cmd =
"qemu-system-aarch64" }` is satisfied either way, so `runner-doctor.sh` said OK.

## Fix

The fix goes in the image, not the host (issues 1457/1482). Add `ipxe-qemu` to
the row's `apt` list. The generated runner Dockerfile picks it up, and
`ci/docker/runner-context.golden/Dockerfile.golden` moves by that one line. No
other image installs this key.

The `dnf`/`pacman` rows are unchanged. Nobody measured whether those packages
ship the ROMs, and no lane uses them.

## Acceptance

After the runner image is rebuilt (`runner-container.sh --build`, by the
operator), a `merge-runtime.yml` `lane=merge` dispatch reports
`CorePinPlacement/zephyr/c` ACCEPT, and its zephyr shard has no real failures.

## Not covered

The `check` still asks only for the binary, so a missing ROM still reads as
provisioned. No `check` form in the index probes a data file outside an SDK
tree. Adding one is a separate change.
