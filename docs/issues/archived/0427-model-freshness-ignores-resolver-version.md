---
id: 427
title: "a SystemModel is 'fresh' when only the RESOLVER changed, so a resolver fix never reaches existing models"
status: resolved
type: bug
area: build
related: [phase-330, phase-336, issue-0382, issue-0285]
resolved_in: stamp_resolver_pin + resolver-pin freshness input
---

`model_provenance_stale` (`ws.rs`) keyed freshness on input file hashes only, so a
resolver fix (node ordering, params, remaps, tiers) that changes the OUTPUT for
byte-identical inputs never invalidated an existing model — `nros sync` exited 0
having done nothing. The recorded `meta.resolver.version` was also bogus (`0.1.0`
while the resolver was v0.1.4): the resolver's own self-stamp is unreliable.

RESOLVED by making the resolver PIN a freshness input:
- `stamp_resolver_pin` overwrites `meta.resolver` with `NROS_PLAY_LAUNCH_SHA` (the
  pin `verify_resolver_pin` already agrees on at sync time) on the staged model
  before it is promoted — so the field carries the real pin, not `0.1.0`.
- `model_provenance_stale` compares the recorded pin against
  `env!("NROS_PLAY_LAUNCH_SHA")` and returns stale on mismatch or when no pin is
  recorded (legacy/pre-fix models), so a resolver change re-resolves every model.
  Skipped when our pin is `"unknown"`, matching `verify_resolver_pin`.

Verified end-to-end: a fresh sync stamps the real SHA (not `0.1.0`); an unchanged
model still skips; tampering the pin re-resolves and restores it. Unit tests
`resolver_pin_change_is_stale` + `missing_resolver_pin_is_stale` added. This
reaches the original symptom — the stale `cpp_multi_node_entry` model (listener
before talker) now re-resolves to the launch's talker-first order.
