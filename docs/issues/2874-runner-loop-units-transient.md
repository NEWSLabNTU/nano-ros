---
id: 2874
title: "Host 241's runner loops run as transient systemd units, so a reboot loses them and their host label"
status: open
type: limitation
area: ci, runners
severity: low
found: 2026-11-23
related: [issue-2663, phase-489]
---
## What
Host 241's two self-hosted runners are kept alive by two TRANSIENT systemd
user units, created with `systemd-run --user`:

- `nros-runner-loop.service` (labels `nros-qemu,nros-sdk-zephyr,nros-big`);
- `nros-runner-loop-queue.service` (labels `nros-queue,nros-sdk-zephyr,nros-big`).

A transient unit lives only as long as the user's systemd manager. A reboot,
or the end of the user session when lingering is off, removes both units.
Nothing starts the runners again, and the merge queue then waits on its
required self-hosted check. The environment goes with the units, including
`NROS_RUNNER_HOST=241`, so a hand restart that copies an older command also
drops the `nros-host-241` routing label (issue 2663).

## How it was found
On 2026-11-23 both units were restarted, one at a time and only while their
runner was idle, to add `NROS_RUNNER_HOST=241`. The loop passes its
environment into every container it starts, so the variable has to be on the
unit; `runner-container.sh --run` alone would start a second container under
the same name. After the restart both runners showed online on GitHub with
`nros-host-241`, and `runner-doctor.sh` passed all four labels on each.

## What the units hold
Both share these settings:

- Working directory: `/mnt/toshiba/aeon/nros-runner/loop-checkout`
- Environment:
  - `NROS_RUNNER_STORE_ROOT=/mnt/toshiba/aeon/nros-runner`
  - `NROS_RUNNER_HOST=241`
  - `HOME=/home/aeon`
  - `PATH`: the operator's login PATH at creation time (the SDK store bins,
    `/opt/ros/humble/bin`, `~/.cargo/bin`, the system dirs).

`nros-runner-loop.service`:

- `NROS_RUNNER_NAME=nano-ros-runner-newslab-server-241`
- Command: `/usr/bin/bash -c "exec scripts/ci/runner-loop-container.sh nros-qemu,nros-sdk-zephyr,nros-big >> /mnt/toshiba/aeon/nros-runner/logs/runner-loop.log 2>&1"`

`nros-runner-loop-queue.service`:

- `NROS_RUNNER_NAME=nano-ros-runner-newslab-server-241-queue`
- Command: `/usr/bin/bash -c "exec scripts/ci/runner-loop-container.sh nros-queue,nros-sdk-zephyr,nros-big >> /mnt/toshiba/aeon/nros-runner/logs/runner-loop-queue.log 2>&1"`

`systemctl --user cat <unit>` prints the live definition, and
`systemctl --user show <unit> -p Environment` prints its exact environment.

## Fix direction
Persistent unit files under `~/.config/systemd/user/`, holding the settings
above with `Restart=` and `WantedBy=default.target`, plus `loginctl
enable-linger` for the user so they start at boot. A PATH stated in the unit,
rather than copied from a login shell, makes the runner's tool set a file
someone can read. Changing the host's service setup is the user's decision.

## Acceptance
Both runners come back online on GitHub, with `nros-host-241`, after a
reboot of host 241 with no hand step.
