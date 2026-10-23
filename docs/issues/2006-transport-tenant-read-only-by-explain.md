---
id: 2006
title: "RFC-0086's transport tenant is read only by `nros config explain`; builds take the transport from `[image.<id>] transport`"
status: open
type: tech-debt
area: [config, cli]
severity: medium
found: 2026-10-23
related: [1037]
---

## Measured

Phase-400 W3 (`2104ebef99`, 2026-08-30) landed RFC-0086 D1/D2: a
`[knobs.transport]` tenant (`kind`, `endpoint`) on the platform and board
descriptors, the `exactly-one-of` / `requires` / `implies` verbs, and
`PlatformsTree::resolve_transport` in
`packages/tooling/nros-platform-config/src/platform_config.rs`.

Checked on `main` 2026-10-23:

- `resolve_transport` has ONE caller outside its own tests:
  `packages/cli/nros-cli-core/src/cmd/config.rs`, the `nros config explain`
  verb. No build script, cmake module or `nros build` stage calls it.
- No tracked `nros-platform.toml` or `nros-board.toml` sets
  `[knobs.transport]` (`git grep -n 'knobs.transport' -- '*.toml'` is empty).
- The transport a build actually gets is RFC-0098 D4's
  `[image.<id>] transport` in `system.toml`. Its implications (`ZPICO_NO_SMOLTCP=1`,
  `NROS_LINK_IP=0`) are applied by `nros-cli-core/src/cmd/leaf_settings.rs`,
  a second implementation that never consults the tenant.

So `explain` describes a resolution no build performs. With the worktree's own
CLI, `nros config explain --platform bare-metal` prints:

```
transport.kind           tcp        builtin
links.tcp                true       implied by transport.kind=tcp
links.serial             false      implied by transport.kind=tcp
drivers.ethernet         true       implied by transport.kind=tcp
net.ip_stack             true       implied by transport.kind=tcp
...
```

The same board builds `examples/mps2-an385-baremetal/rust/serial-listener`,
whose `system.toml` states `transport = "serial"`, as a serial image. `explain`
takes a platform and a board and no image, so it cannot report that image's
transport, and the nine "implied" rows it prints reach no compiler.

RFC-0086 D6 makes provenance "the acceptance test" of the design. A provenance
printout that disagrees with the build is the failure that decision exists to
prevent.

## What closing it means

Pick one transport mechanism and give it one reader. Either:

- **retire the tenant**: delete `[knobs.transport]`, `resolve_transport` and its
  `explain` rows, and record in RFC-0086 that D1/D2 were superseded by RFC-0098
  D4; or
- **route the image's statement through it**: `nros build` / `nros sync` resolve
  `[image.<id>] transport` with `resolve_transport`, so the verbs and the
  `explain` rows describe what the build gets, and `leaf_settings.rs`'s own
  implication table goes.

Which one is a design decision for RFC-0086's owner. Until then RFC-0086 is
Stable for D3–D6 only.

**Home:** none. Phase-400, which landed W3, was archived on 2026-10-23 with
this issue as its one open design question.
