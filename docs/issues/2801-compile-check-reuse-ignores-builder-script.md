---
id: 2801
title: "A compile-check row is reused after an edit to the builder that compiles it, because its input signature does not hash the builder"
status: open
type: bug
severity: low
area: [ci, fixtures]
related: []
found: 2026-11-21
---

## What happens

`scripts/build/compile-check-fixtures.sh` reuses a staged row when its
`.inputsig` matches what `scripts/build/compile-check-signature.sh` computes
for it ("reused … — inputs unchanged (.inputsig)"). The signature hashes the
manifest record, the row's source tree, the nros CLI's codegen fingerprint
and the launch resolver's identity. It does not hash the builder functions
in `compile-check-fixtures.sh` that stage and compile the row
(`stage_tree`, `stage_and_check`, `stage_and_check_verdict`, …).

So an edit to HOW a row is built leaves every row of that builder reading
fresh. A `cargo-check-verdict` row keeps its old `verdict.*` files, and the
test that reads them reports the old builder's outcome.

## Measured

While fixing issue 2777, the `cargo-check-verdict` builder gained a
`--config build/<image>/nros-cargo.toml` argument. Re-running the lane for
`main_macro_resolves_from_inputs` and `main_macro_rebuilds_on_model_touch`
printed `reused … — inputs unchanged (.inputsig)` for both, and
`nros-tests::linux_main_macro_misuse` still failed on the verdict the
previous builder had recorded (the `nros::main!` store refusal). Removing
the two rows' `.inputsig` rebuilt them.

A fresh checkout (CI) is unaffected, since nothing is reused there. A local
`just ci gate` run after a builder edit reads the stale verdicts.

## Fix direction

Fold the builder into the signature: at least the hash of
`scripts/build/compile-check-fixtures.sh` and the helpers it sources, or
per-builder the functions that builder runs, so that an edit to one builder
does not re-stale the rows of every other one. Whichever is chosen, it moves
every existing `.inputsig` once.
