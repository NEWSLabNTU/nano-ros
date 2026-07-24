---
id: 247
title: "nros codegen-system PIO/PX4 ahead-of-vendor arms emit literal TODO(E.3) stub output"
status: open
type: limitation
severity: low
area: cli
related: [rfc-0039, rfc-0040]
---

## Finding (release-prep audit 2026-07-24)

`packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (~line 909): the
`AheadOfVendor::Pio` and `AheadOfVendor::Px4` arms emit a JSON document whose
payload is a literal TODO string:

```rust
AheadOfVendor::Pio => "TODO(E.3): augment PlatformIO library.json with transport + framework",
AheadOfVendor::Px4 => "TODO(E.3): emit PX4 board overlay flipping CONFIG_MODULES_NROS_<NAME>=y",
```

A user driving `codegen-system` at a PlatformIO or PX4 target gets a
well-formed file whose content is a stub — it looks like output but does
nothing. Release-facing risk: the verb *appears* to support those targets.

## Fix directions

Either implement the two arms (PlatformIO library.json augmentation; PX4
board overlay emission — RFC-0039), or make the arms fail loudly
("PlatformIO/PX4 codegen-system output not yet implemented") instead of
emitting a plausible-looking stub file. Loud failure is the cheap
release-safe option.
