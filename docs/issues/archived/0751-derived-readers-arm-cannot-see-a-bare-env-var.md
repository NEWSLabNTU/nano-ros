---
id: 751
title: "`check-kconfig-knob-forwarding`'s derived-reader arm matches on the NAME APPEARING, so a bare `env::var` on a forwarded knob satisfies it — the defect it exists to catch"
status: resolved
type: tech-debt
severity: medium
area: build/zephyr
related: [issue-0460, issue-0749, issue-0196]
---

## Finding

The gate holds its two reader classes to different standards.

**Tabulating `READERS`** must have a `KCONFIG_KNOBS` table *and* call
`nros_zephyr_build::(knob_usize|dotconfig_usize)` — both explicitly checked,
with the stated reasoning that "a table nobody consults is the same silence with
extra steps."

**`DERIVED_READERS`** get neither check. Their whole test is the env name
appearing anywhere in the file:

```sh
if grep -qF "\"$knob\"" "$f"; then found=1; break; fi
```

A bare `env::var("NROS_MAX_PARAMETERS")` satisfies that. On a Zephyr Rust image
it also reads the crate DEFAULT whatever Kconfig says, because that lane
inherits none of cmake's `set(ENV{...})` exports — which is issue 0460, the
exact failure this gate exists to prevent.

## It is not hypothetical, and #0749 nearly demonstrated it

`nros-params/build.rs` read `NROS_MAX_PARAMETERS` with a plain `env::var` while
the cmake side forwarded it (#0749 follow-up, `c64671527`). The gate caught that
— but only because the file was not yet listed as a reader. The fix necessarily
added it to `DERIVED_READERS`, and from that moment the same defect in that file
would pass.

So the gate caught this instance by luck of registration order, and registering
a file is what removes it from scrutiny. That is issue 0196's shape: coverage
narrower than the rule enforced.

## Measured before fixing

Whether the hole was occupied:

| derived reader | sanctioned helper | bare `env::var("NROS_*")` |
| --- | --- | --- |
| `nros-node/build.rs` | 2 | 0 |
| `nros-rmw-xrce-cffi/build.rs` | 1 | 2 |
| `nros-params/build.rs` | 2 | 0 |

The two bare reads in `nros-rmw-xrce-cffi` are `NROS_LINK_IP` and
`NROS_XRCE_CUSTOM_TRANSPORT_MTU`, and **neither is forwarded by the cmake side**
(`_nros_resolve_knob` names neither). So no live instance — latent, not a bug.

## Fix

Two arms added to `scripts/check-kconfig-knob-forwarding.sh`:

1. **Per knob, in the matching loop:** a forwarded knob found in a reader must
   not appear as `env::var("<KNOB>")` in that file. That is the precise defect
   shape and it is what `nros-params/build.rs` looked like before `c64671527`.
2. **Per derived reader:** it must call
   `nros_zephyr_build::(knob_usize|dotconfig_usize)`, the same requirement the
   tabulating readers already carry. A file listed as a reader that resolves
   nothing from `$DOTCONFIG` names knobs without reading them.

Both falsified against the real gate, not only by inspection:

* reintroducing the pre-fix shape (`env::var("NROS_MAX_PARAMETERS")` in
  `nros-params/build.rs`) →
  `[FAIL] … reads forwarded knob NROS_MAX_PARAMETERS with a bare env::var`;
* adding a derived reader that names a knob and calls no helper →
  `[FAIL] … is listed as a derived reader but never calls the shared
  nros_zephyr_build fallback`;
* clean tree → `OK — 26 forwarded knob(s)`.

A first attempt at the second case used `nros-rmw-zenoh/build.rs` as the probe
and reported nothing. That file is a tabulating READER and already calls the
helper — the gate was right and the probe was wrong, which is worth recording
because a silent falsification attempt reads exactly like a gate that cannot
fail.
