---
id: 1279
title: "`just setup zephyr` skips the whole SDK setup when the WORKSPACE
  exists, but cmake registration is per-USER state — a host can hold a
  complete, unpacked, unusable SDK that the verb cannot repair"
status: resolved
type: bug
area: build, setup
severity: medium
found: 2026-09-11
related: [issue-1274, issue-1276, rfc-0095, phase-449]
resolved_in: "phase-449 W8 — registration is gated on itself"
---

## What this is

`just/zephyr-setup.just`:

```sh
if [ -d "$WORKSPACE/zephyr" ] && [[ "$ARGS" != *--force* ]]; then
    echo "Zephyr workspace already present at $WORKSPACE"
    echo "To reinstall, run: just zephyr setup --force"
else
    NROS_ZEPHYR_MANIFEST=… ./scripts/zephyr/setup.sh $ARGS
fi
```

`scripts/zephyr/setup.sh` does more than fill the workspace. Among other things
it runs the Zephyr SDK's own `setup.sh -h -c`, whose `-c` writes the cmake
package registry entry:

```
~/.cmake/packages/Zephyr-sdk
```

That is per-USER state, not per-workspace. The gate in front of it asks about
the workspace. So a host whose workspace is present and whose registry entry is
absent is a host `just setup zephyr` cannot fix — it prints "already present"
and returns 0.

## Why the state is reachable

Two ways, and the second is not exotic:

* the workspace is provisioned, and `~/.cmake` is later lost or cleared;
* the workspace lives in a persistent store and `~/.cmake` does not. That is
  exactly a contained CI runner: `~/.nros/workspaces/zephyr/3.7` is a volume,
  and `~/.cmake` was in the container's writable layer, which `--ephemeral`
  destroys after one job. Every container would have started with a complete,
  unpacked SDK and no registration.

## Why it matters more than it looks

`runner-doctor` catches it and says what it costs:

```
[MISSING] Zephyr SDK 0.16.8 is not registered with cmake
          find_package(Zephyr-sdk) reads ~/.cmake/packages/Zephyr-sdk/;
          an unregistered SDK fails at configure, not at download.
          Run the SDK's own registration:
            (cd …/zephyr-sdk-0.16.8 && ./setup.sh -h -c)
```

"Fails at configure, not at download" is the whole point: the failure surfaces
in a build, far from the provisioning step that should have prevented it. And
the remedy the doctor prints is the SDK's own script, NOT a nano-ros verb —
which is an admission that the verb cannot do it.

## Fix

Split the skip. The workspace check should gate the workspace work (`west init`
/ `west update`), not the steps that are idempotent and cheap and about the
user's own environment. SDK registration is one of those: it is a few
milliseconds, it is safe to repeat, and it is the difference between an SDK that
builds and one that does not.

Concretely, one of:

* run the SDK's `setup.sh -c` unconditionally near the end of the recipe (the
  `-h` half, installing host tools, is the expensive part and can stay behind
  the skip — note it FAILED in a container, so it wants its own look);
* or give the registration its own verb the doctor can name, so its remedy line
  points at nano-ros rather than at a vendor script.

Prefer the first. A second verb is another thing to remember, and the reason
this bug exists is that a step got attached to the wrong condition.

## Worked around, not fixed

The contained runner registers the SDK by hand once, and `~/.cmake` is now one
of its persistent stores, so it survives (PR #882). Resetting that store would
need the registration re-run by hand. That is a workaround at the wrong layer
and is recorded here so it does not become the answer.

## Note for whoever is revising provisioning

This is the same shape as issue 1274 — a step gated on a condition that is not
the one it depends on — and the same shape as issue 1276, where the installer
read a field instead of asking the derivation. Worth fixing together with
whatever else touches `scripts/zephyr/setup.sh`.


## Resolution (phase-449 W8, 2026-09-18)

`scripts/zephyr/ensure-sdk-registered.sh` is the registration step, gated on
ITSELF: it reads `~/.cmake/packages/Zephyr-sdk/` for an entry whose CONTENT is
this SDK's `cmake` directory, and registers only when there is none.

Keying on the content rather than on "the directory is non-empty" is what makes
it per-ARTIFACT, which is what this issue asks for: a host registered for a
DIFFERENT SDK version has a non-empty registry directory and still cannot build
this line.

It runs `-c` and never `-h`, taking the issue's own preferred option. `-c`
writes the registry entry; `-h` installs host tools, is the expensive half, and
is reported here to fail in a container. A step that is cheap and always safe
must not inherit the gating of one that is neither — which is this bug stated
once.

### The same defect existed twice, one layer apart

The issue quotes the outer one: `just/zephyr-setup.just` skips all of
`scripts/zephyr/setup.sh` when the WORKSPACE exists. `setup.sh` then skips
`install_sdk` — the function that runs the SDK's registration — when the SDK
DIRECTORY exists. Fixing only the verb would have left a host with an unpacked,
unregistered SDK unrepairable by `setup.sh` itself. Both call the helper now,
unconditionally; `--skip-sdk` still skips it, because that flag is the caller
saying they manage the SDK themselves.

### Acceptance, measured end to end

`~/.cmake/packages/Zephyr-sdk/` emptied, then `just zephyr setup` with no flags:

```
Zephyr workspace already present at zephyr-workspace
ensure-sdk-registered: ...zephyr-sdk-0.16.8 is unpacked but NOT registered with cmake; registering.
ensure-sdk-registered: registered ...zephyr-sdk-0.16.8 with cmake.
```

The workspace skip still fires — it is correct, it is just no longer in front of
this — and every source step reports `already present (skip)`, so nothing was
re-fetched. No `--force`.

The version comes from `zephyr/SDK_VERSION`, which the doctor block beside it
already reads, rather than a fourth hardcoded `0.16.8`. The call is not
`|| true`: this issue is about a failure being invisible until a build dies
inside `FindZephyr-sdk.cmake`, and swallowing it would rebuild that defect one
line lower.

### The workaround this replaces

The contained runner registered the SDK by hand once and made `~/.cmake` a
persistent store (PR #882). That stays harmless, but it is no longer what makes
the state recoverable, and resetting the store no longer needs a manual step.
