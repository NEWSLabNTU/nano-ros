---
id: 663
title: "`nros setup --tool cyclonedds` installs `idlc` and nothing puts it on PATH, so every Cyclone lane skips and tells the user to run the DEV recipe"
status: resolved
type: bug
severity: high
area: build/toolchain
related: [issue-0486, issue-0625, issue-0657, issue-0650]
---

## Symptom

Following the user-facing path on a host with no ROS:

```
$ nros setup --tool cyclonedds       # succeeds; store holds bin/idlc
$ just threadx_riscv64 doctor
  [INFO] idlc not found — ThreadX-RV64 Cyclone fixtures will skip.
         Provision with: just cyclonedds setup …
```

The tool is installed. The lane cannot see it, and the remedy it prints names an
IN-REPO developer recipe — advice a user of the released project cannot follow.

## Cause 1 — provisioned but not reachable

`activate.sh` puts a store bin dir on `PATH` only when it holds a whitelisted
tool. The whitelist was a hand-written `[ -x … ] || [ -x … ]` chain, and `idlc`
was not on it.

This is the third instance of one bug. The chain's own comment describes the
second: *"`nros setup --tool espflash` succeeded and the pack step still
skipped, because nothing put the store bin dir on PATH"* — and `genromfs` before
that. Each was fixed by appending one more `-x` test to a list nobody thought of
as a list.

Worse, there were TWO chains — `activate.sh` and `activate.fish` — and they had
already drifted: `espflash` was in the bash one only, so the same provisioned
host behaved differently depending on the shell. The parity gate
(`check-activate-shells`) compares exported VARIABLES and never looked at this.

**Fix:** the whitelist is now DATA — `scripts/sdk-path-tools.txt`, read by both
shells. Drift is structurally impossible, and adding a tool is one line in a
file that says why the list exists.

## Cause 2 — the interfaces the checkout ships are not on the search path

With `idlc` reachable, the Cyclone fixtures got further and then failed:

```
Could not find interface file for std_msgs: msg/String.msg
  Searched:
    <example>/msg/String.msg
    AMENT_PREFIX_PATH/share/std_msgs/msg/String.msg
    <repo>/share/nano-ros/interfaces/std_msgs/msg/String.msg
```

The third tier names an INSTALLED layout. A source checkout does not have it —
in-tree the same files ship at `packages/cli/interfaces/`, which the compat stub
`_NrosFindRosMsgPackage.cmake` already knew about and the resolver's tier list
did not. So on any host without ROS, cmake-driven interface generation could not
find files that were sitting in the repo it was searching.

**Fix:** a fourth tier, in both the resolver (`_nros_resolve_interface_file`) and
the GLOB fallback, plus the path added to the error message — so a future miss
names every place actually consulted.

## Verification

* `nros setup --tool cyclonedds` → `just threadx_riscv64 doctor` reports
  `[OK] idlc (Cyclone DDS IDL compiler): ~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc`.
* Both shells resolve it (`bash` and `fish`), from the shared list.
* `just threadx_riscv64 build-fixtures` → rc 0 WITH the Cyclone cells built:
  `examples/qemu-riscv64-threadx/{c,cpp,rust}/listener/build-cyclonedds/…`.
* The lane's remedy now names the user path first (`nros setup --tool cyclonedds`)
  and the in-repo recipe as the alternative.

## What this uncovered

The Cyclone cells for this board now BUILD and RUN, and the runtime tests fail —
see issue 0664. They had been skipping on every host that did not hand-build the
dev copy, so nothing had ever exercised them. A skip that nobody can clear is
indistinguishable from a pass.
