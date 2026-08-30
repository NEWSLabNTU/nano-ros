---
id: 423
title: Borrowed-view RUNTIME e2e (C/C++) was orphaned + bit-rotted; runtime coverage lost
status: resolved
type: tech-debt
area: testing
related: [rfc-0033, phase-329]
resolved_in: phase-329 W5 follow-up
---

The borrowed-view (RFC-0033) RUNTIME proofs `tests/borrowed_{c,cpp}_e2e.sh` were
orphaned (no lane/recipe/CI ran them) and bit-rotted (two breakages), so they were
first DELETED. RE-ESTABLISHED as a build-stage fixture + Rust consumer (the phase-329
W5 pattern), fixing both rots:

1. **`<nros/platform.h>` moved to nros-platform-api** (RFC-0042 D1) — added to the
   compile `-I` set.
2. **The `nros_config_variant_sz_<hash>` guard.** A standalone `cargo build -p nros-c`
   can't size the executor (probe → 0, `nros-build-helpers/src/c.rs:127`), so its
   archive never defines the anchor the config header imports — the link failed
   `undefined reference`. But borrowed views read the CDR buffer via `nros_serdes`,
   NOT the executor's opaque storage, so the guard is a false constraint here. The
   anchor is emitted `__attribute__((weak))` precisely so a consumer may provide its
   own that merges, so the recipe reads the symbol out of the freshly-built config
   header and links a matching weak anchor (compiled as C — a C++ file-scope `const`
   has internal linkage and can't be weak). Header + archive are one build, so their
   stub sizes agree.
   A third rot surfaced on the C++ side: the `ffi_wrapper.rs` prelude had drifted
   without the `fixed_str()` helper the current codegen calls — restored from
   `cmake/ffi_lib_rs.in`.

Now: `scripts/build/borrowed-e2e-fixture.sh` links `build/borrowed-e2e/borrowed_{c,cpp}_e2e`;
`tests/borrowed_e2e.rs` RUNS them (no compile-at-test) and asserts every view aliases
the CDR buffer; `just check borrowed-e2e` wires build+test into `check-build`. Both
pass (C + C++). No longer a negative-diagnostic-registry member.
