---
id: 752
title: "Five sizing knobs reached build.rs only from the calling shell's environment, so a bare `ninja` rebuilt the image at crate defaults — silently for the subscription-slot pool"
status: resolved
type: bug
area: zephyr
related: [issue-0316, issue-0749, issue-0739, issue-0751]
---

# 0752 — knobs that survive `just build` and not `ninja`

Found by the ASI safety-island consumer sizing a four-node image into the
S32K344's 320 KiB. #0749 resolved the nros-node sizing class into the curated
cargo environment; five knobs outside that class were still reachable **only**
by being exported in the environment of whatever shell ran ninja:

```
NROS_EXECUTOR_ARENA_SIZE      NROS_RMW_SUBSCRIBER_SLOTS
ZPICO_SUBSCRIBER_RING_DEPTH   ZPICO_MAX_LARGE_SUBSCRIBERS   ZPICO_SUBSCRIBER_LARGE_SIZE
```

## Why that is not merely inconvenient

`nros_cargo_build()` bakes `${_nros_knob_env}` into the cargo custom command, so
a knob with a `_nros_resolve_knob` row rides `build.ninja` and is reproducible.
A knob without one is not in that list at all — it reaches `build.rs` through
plain environment inheritance. The consumer's recipe exported all five, so the
image it built was correct. The same configured tree rebuilt by `cd build && ninja`,
or by `west build -d build` from a plain shell, silently produced a **different
image**: `cargo:rerun-if-env-changed` fires on the variable's disappearance and
the crate recompiles at its default.

Verified against the consumer's generated `build.ninja` before the fix —
`NROS_EXECUTOR_MAX_CBS`, `NROS_SUBSCRIPTION_BUFFER_SIZE`, `NROS_EXECUTOR_MAX_NODES`
and `NROS_MAX_PARAMETERS` present; all five above absent.

Most of the reversion fails loudly on a tight part: the derived arena and the
128 KiB default `LARGE_PAYLOADS` overflow RAM and the link stops. One does not.
`NROS_RMW_SUBSCRIBER_SLOTS` drops from a consumer's 12 back to 8, and the 9th
`create_subscription` returns `BAD_ALLOC`, surfacing as an opaque
`SubscriberCreationFailed` at boot — the same failure shape issue 0269 already
fixed once when the pool was a hardcoded 4. A silent revert of a fix is worse
than the original bug, because the fix is in the tree and looks applied.

This is issue 0739's thesis one level down: it is not enough for a knob to
exist and be documented, it has to be *reachable by the mechanism the build
actually uses*.

## Fix

Kconfig rows + `_nros_resolve_knob` for all five, so they resolve env-wins and
bake into the cargo command like the rest.

- `NROS_SUBSCRIBER_RING_DEPTH` (4) → `ZPICO_SUBSCRIBER_RING_DEPTH`
- `NROS_MAX_LARGE_SUBSCRIBERS` (2) → `ZPICO_MAX_LARGE_SUBSCRIBERS`
- `NROS_SUBSCRIBER_LARGE_SIZE` (16384) → `ZPICO_SUBSCRIBER_LARGE_SIZE`
- `NROS_RMW_SUBSCRIBER_SLOTS` (8) — backend-independent; the no_std slot path is
  in the cffi adapter, not a transport, so it resolves outside the zenoh block
- `NROS_EXECUTOR_ARENA_SIZE` (0 = derive)

The arena is **tri-state** and needs the care. `nros-node/build.rs` derives a
size when the variable is absent, so forwarding a literal `0` would hand it a
zero-byte arena instead of the derivation. It resolves only when someone chose a
value — Kconfig non-zero, or an explicit environment override:

```cmake
if(DEFINED ENV{NROS_EXECUTOR_ARENA_SIZE}
   OR NOT "${CONFIG_NROS_EXECUTOR_ARENA_SIZE}" STREQUAL "0")
    _nros_resolve_knob(NROS_EXECUTOR_ARENA_SIZE
        "${CONFIG_NROS_EXECUTOR_ARENA_SIZE}")
endif()
```

## Verified

On `mr_canhubk3/s32k344`, zenoh, the ASI four-node island:

1. All five now appear in `build.ninja`; the image is byte-identical to the
   env-exported build (RAM 273,072 / DTCM 98,264).
2. `NROS_RMW_SUBSCRIBER_SLOTS` unset in the shell, bare `ninja` in the
   configured tree: the pool stays at `0x3000` (12 slots). Before the fix this
   is where it reverted to 8.
3. Tri-state: with neither the environment nor Kconfig setting the arena,
   `NROS_EXECUTOR_ARENA_SIZE` is absent from `build.ninja` and `build.rs`
   derives 74,240 from `MAX_CBS=4` — not 0. That build then fails at link with
   `RAM overflowed by 187608 bytes`, which is the correct outcome for an
   all-defaults build on a 320 KiB part.
