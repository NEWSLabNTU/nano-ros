---
id: 2917
title: "A Zephyr 4.4 Cyclone build on a humble host is told to source ROS's setup.bash, which cannot work under Python 3.12"
status: resolved
type: bug
area: [zephyr, cyclonedds, tooling]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [issue-1457, issue-2187, issue-2821, issue-2902]
---

## What happens

Host 142 measured it on 2026-11-23: a Zephyr 4.4 image with Cyclone DDS
failed at the msg-to-IDL step with

    error: rosidl_adapter is not importable by this build's interpreter.

The Zephyr 4.4 tree needs Python >=3.12, so the build runs under the line's
3.12 venv. Humble's `rosidl_adapter` lives under `/opt/ros/humble/lib/python3.10`
and is built for 3.10, so the build's interpreter can never import it. On a
jammy/humble host, ROS's copy cannot serve a 4.4 build at all.

## The question asked, and the answer

The question was whether to run rosidl under ROS's interpreter and west under
the 3.12 venv, or to declare 4.4 with Cyclone unsupported on humble.

Neither is needed. `msg_to_cyclone_idl.py` already has a second road, the
vendored rosidl (`nros setup --source rosidl`, in the store as
`sources/rosidl/humble-<rev>`), run under the build's own interpreter with
`PYTHONPATH` pointing into the clone. Issue 2187 chose that road for this line
and added the `cyclone-idl` group to the venv check. Measured on host 241: a
fresh Python 3.12.14 venv with `catkin_pkg`, `empy==3.3.4`, `lark` and `PyYAML`
imports `rosidl_adapter.cli` from the store's clone.

## What was actually wrong

Three things kept that road from being found:

1. **The `cyclone-idl` group was one module short.** The vendored `rosidl_cli`
   imports `yaml`. `scripts/check-python-deps.py` listed only `catkin_pkg`,
   `em` and `lark`, while `_VENDORED_PY_DEPS` in `msg_to_cyclone_idl.py`
   listed all four. Every interpreter that also passed the `west` group had
   PyYAML anyway, so the gap hid. A venv provisioned for this group alone
   still failed the import.
2. **The refusal sent a 4.4 build down the road that cannot work.** It said
   ROS's copy was "present but not on PYTHONPATH" and offered
   `source /opt/ros/<distro>/setup.bash` as a fix. Under 3.12 that is never a
   fix.
3. **Host 241's 4.4 venv lacked the group.** It was made by hand, without
   `NROS_ZEPHYR_CREATE_VENV=1`, whose `--create` installs `cyclone-idl` along
   with `west`. `provision-py312-venv.sh` reported this as a warning, which is
   the intended behaviour; it is host state, not code.

## Resolution

Fixed 2026-11-24.

- The `cyclone-idl` group names `PyYAML`, so it matches `_VENDORED_PY_DEPS`.
- `just zephyr setup` provisions `--source rosidl` (about 3 MB) with the other
  sources it already fetches, so the vendored road exists on any host that ran
  setup.
- When ROS's copy is built for a different Python than the build's, the
  refusal says so, names both versions and this interpreter's path, points at
  the vendored copy and its missing deps, and drops the `source setup.bash`
  line. A build on ROS's own Python gets the old text, unchanged.

Measured on host 241:

- With a bare 3.12 venv, the refusal names python3.10 against python3.12, the
  store's clone and all four missing deps.
- With the four deps installed, the script starts normally.

The 4.4 + Cyclone road on a humble host is therefore supported. It needs
`nros setup --source rosidl` and the `cyclone-idl` group in the line's venv,
both of which `NROS_ZEPHYR_CREATE_VENV=1 NROS_ZEPHYR_VERSION=4.4 just zephyr
setup` provides. No full 4.4 Cyclone image was built for this record.
