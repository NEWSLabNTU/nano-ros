---
id: 187
title: "examples_canonical_shape red: 12 stm32f4/baremetal rust leaves violate the §212.L.4 class-prefix rule"
status: resolved
type: tech-debt
area: testing
related: [phase-287, phase-212]
resolved_in: "2026-07-14 lint normalization (crate-ident prefix)"
---

## Summary (as filed)

The phase-287 W7 lint (`examples_canonical_shape`) failed on rust leaves
whose `class` prefix is the crate ident (underscores) while the lint demanded
the verbatim Cargo package name (hyphens) — 12 leaves at filing, **22** by
resolution (more `.component` leaves landed in between).

## Root cause — the lint, not the leaves

The demanded form was literally unrepresentable: a Rust path cannot contain
hyphens, so every hyphen-named package could never satisfy the verbatim
check. The consumer defines the canonical mapping the other way:
`ComponentLinkage::resolved_crate_name` (nros-cli-core `orchestration/
config.rs`) derives the crate as *"the ROS package name with `-`→`_`
(package.xml ⇒ Cargo crate convention)"* — the 22 leaves' metadata matched
exactly what the code consumes. The older sibling lint
(`example_shape::component_class_strings_match_package_name`) already
compared the normalized form; only the new W7 walker compared verbatim. The
§212.L.4 spec text ("pkg dir name MUST match the prefix") supports identity,
not a hyphenated literal.

## Fix

`examples_canonical_shape.rs` §212.L.4 check: the expected prefix is now
`package.name.replace('-', "_") + "::"` (identical to the verbatim name for
underscore-named packages), with the message naming the crate-ident rule.

Verified: the lint + the full `example_shape` suite 10/10 green on the
untouched tree; seeded a genuinely wrong prefix
(`class = "wrong_crate::Talker"`) → the lint fires with the new message;
restored → green.
