---
id: 930
title: "The built QEMU can be older than the commit `third-party/qemu/qemu` pins, and nothing says so"
status: open
type: bug
area: testing, tooling
related: [issue-0196, issue-0917]
---

## What happens

`nros_tests::qemu::qemu_system_arm_path()` prefers `build/qemu/bin/qemu-system-arm`
— the patched build — over anything on `$PATH`. That binary is produced once by
`just qemu::build` and then simply reused. Nothing re-checks it against the
commit the superproject pins.

Found while re-verifying [[issue-0906]]:

    $ build/qemu/bin/qemu-system-arm --version
    QEMU emulator version 11.0.0 (v11.0.0-3-gdbd1049b06)

    $ git -C third-party/qemu/qemu log --oneline -1
    729262e975 hw/net/lan9118: flush queued packets when RX is enabled

The binary predates the pin by exactly the commit that changes LAN9118 RX
behaviour — the device every mps2-an385 test drives, and the subject of
[[issue-0917]].

## Why it matters more than the version string suggests

We patch QEMU precisely because emulator behaviour is load-bearing for these
tests. A stale build therefore does not fail loudly; it produces DIFFERENT
RESULTS, silently, on an emulator nobody chose. That is the museum-binary class
the fixture-freshness gates exist for ([[issue-0196]]), one layer lower, and the
one layer that currently has no gate:

* `check::submodule-drift` compares the CHECKOUT against the pin. It caught the
  drift that led here, and it is why the pin is now correct — but a correct
  checkout with a stale binary is exactly the state that produced the version
  mismatch above.
* `check::tier-preconditions` enumerates submodules, the CLI and fixtures. The
  built QEMU is in none of those lists, so a full precondition run reports
  everything ready while the emulator is two commits behind.

## The shape of the fix

Same one the CLI and fixtures already use: record what the artifact was built
from, and compare. A stamp beside the binary holding the submodule commit, and
a precondition entry that reads it, with the remedy `just qemu::build`. The
existing text already gets the ordering right — submodules, then CLI, then
fixtures — and QEMU belongs immediately after submodules, since a pin bump is
what invalidates it.

## Acceptance

* Bumping the `third-party/qemu/qemu` pointer makes `check::tier-preconditions`
  report the built QEMU as stale, naming `just qemu::build` as the remedy.
* A test run cannot silently use a QEMU older than the pin.
* The check is a stamp comparison, not a rebuild: `just qemu::build` is
  expensive and must stay opt-in.
