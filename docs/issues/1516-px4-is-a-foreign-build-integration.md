---
id: 1516
title: "`examples/px4/` is a foreign-build integration, not an example layout —
  the exception is real and undocumented"
status: open
type: tech-debt
area: examples, docs
severity: low
found: 2026-09-27
related: [rfc-0026, rfc-0098, issue-0356]
---

## What this is

A layout survey of `examples/` found two canonical shapes — a standalone leaf
`<platform>/<lang>/<example>` (RFC-0026) and a colcon workspace with a generated
entry (RFC-0098 D9) — and `examples/px4/` matches neither. It looks like an
outlier to be migrated. It is not one, and the reason should be written down
where the next survey will read it, because the cost of "unifying" it is a
firmware tree that PX4's build cannot find.

Measured shape:

```
examples/px4/
  cpp/bridge/    ffi/{Cargo.toml,build.rs,src/lib.rs}
                 src/modules/nros_uorb_bridge/{CMakeLists.txt,Kconfig,NrosUorbBridge.cpp}
  cpp/firmware/  src/modules/nros_uorb_demo/{CMakeLists.txt,Kconfig,NrosUorbDemo.cpp}
  rust/companion/{offboard-companion,px4-probe,px4-stub}/
                 {Cargo.toml,package.xml,src/main.rs}
```

Three ways it departs, each with a cause:

1. **`src/modules/<name>/{CMakeLists.txt,Kconfig}` is PX4's layout, not ours.**
   Those trees are consumed by PX4's own build via
   `EXTERNAL_MODULES_LOCATION`, which mandates the directory shape and the
   `Kconfig` beside the module. They are copy-INTO-PX4 sources, not nano-ros
   applications, so there is nothing for `nros build` to generate and no
   `system.toml` to carry.
2. **The sub-directory axis is the transport case, not the language.** Its own
   README says so: PX4 is integrated on its two native messaging surfaces —
   in-firmware uORB modules (C++) and an XRCE-DDS companion (Rust) — so `cpp/`
   and `rust/` name *which surface*, and the language follows from it rather
   than the other way round. A reader who assumes the usual language level will
   look for the missing `cpp/` companion and the missing `rust/` firmware
   module, neither of which can exist.
3. **No RMW axis at all.** The in-firmware modules are uORB-only (the Rust uORB
   backend was retired in phase-115.K.4); the companion speaks XRCE-DDS to
   `uxrce_dds_client`. So the `<rmw>` coordinate every other example carries is
   not a free choice here.

`rust/companion/*` are the three that *could* be moved — they are ordinary host
cargo bins with a `package.xml` and no `system.toml`, so `px4/rust/<example>`
would be well-formed. That move would delete the one level that records the
transport case, to buy uniformity with leaves that do not share PX4's
constraints. Not worth it; recorded here so the trade is a decision rather than
a recurring question.

## What to do

Documentation only:

- a short "PX4 is a foreign-build integration" paragraph in
  `examples/README.md`, next to the layout table, naming
  `EXTERNAL_MODULES_LOCATION` as the constraint and the transport axis as the
  reason for the sub-dirs;
- the same sentence in whatever layout taxonomy phase-470 lands, so the class
  list says *what it is* rather than leaving it in a residual bucket.

## Acceptance

- `examples/README.md` states the exception with its cause.
- A survey following the layout taxonomy reaches `examples/px4/` and is told why
  it is excluded, without reading this issue.
- No files under `examples/px4/` move.
