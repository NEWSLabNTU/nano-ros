# nros-serdes-packed

The reference **schema-driven** serialization provider (RFC-0088 D7,
phase-421 W5). Family `serdes` under RFC-0087.

`packed` is a **test vehicle, not an interop claim.** Nothing else speaks it and
no backend selects it. It exists so the `impl = "schema"` strategy has a subject
that actually exercises the schema walk: uORB's wire is the PX4 struct verbatim
so it walks nothing, and CDR is the `impl = "codegen"` case this strategy is the
alternative to.

The whole implementation is one `SchemaSink` and one `SchemaSource` — roughly
150 lines of primitive encode/decode with no recursion and no type knowledge.
The walk over `&'static [Field]` lives in `nros_serdes::walk`, once. That is the
claim D7 makes and this package is the measurement of it: **a custom format
costs no codegen work.**

## Wire

Little-endian, byte-packed, no padding. `u32` length prefix on strings (no NUL)
and sequences; fixed arrays carry no count; structs carry nothing around their
fields. Every one of those differs from CDR, deliberately — see the table in
`src/lib.rs`.

## Round-trip coverage

`packages/testing/nros-tests/tests/schema_serializer_round_trip.rs` transcodes
every committed generated message in `packages/interfaces/*` through this
provider and back, in both CDR encodings, and asserts byte equality.
