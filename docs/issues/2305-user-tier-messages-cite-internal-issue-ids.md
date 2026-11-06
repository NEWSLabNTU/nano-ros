---
id: 2305
title: "User-tier messages cite internal issue ids (`(issue 1352)`) where they should say what to do; there is no policy separating them from contributor-tier refusals"
status: open
type: tech-debt
area: cli, ux, logging, build
related: [phase-537, rfc-0117, 2270]
found: 2026-11-06
---

**Home: none yet.** Found by the phase-537 W5 book session (PR #3187).
Issue 2270 is one measured instance (the default talker's configure and
boot). This issue is the rule.

## Symptom

An ordinary `nros build` of a C or Rust image on zenoh printed this cargo
warning, from `packages/rmw/zenoh/nros-rmw-zenoh/build.rs`:

```
the parameter family's requests are not priced: the image declares no `params:`.
Its inbox slot keeps N B; a larger `set_parameters` is dropped with no reply.
Declaring the parameters in the contract derives it (issue 1352)
```

That warning reaches every user who builds a zenoh image with parameter
services and no contract `params:`. It is user-tier output. "(issue 1352)"
gives the reader nothing to act on: the tracker is `docs/issues/` in a
checkout the user may not have, and "the contract" is not named as a file.

## Why ids are not simply stripped

Naming the issue in a refusal is a deliberate convention here. CLAUDE.md and
the gates are full of "refuses, naming issue N", and for a CONTRIBUTOR
reading a gate, a build-script panic or a `nros dev` verb, the id is the
fastest way to the history. The problem is the audience, not the id.

## Proposal

Two tiers, matching RFC-0117's command tiers:

- **User tier**: output a user can reach with no checkout. That covers the
  user-tier verbs (RFC-0117 D2), warnings and refusals from build scripts
  and cmake on an ordinary user build, and runtime logs of a default image.
  These messages say what is wrong and what to do: which file and key, or
  which knob. Where more is needed they point to a book page or URL. An
  internal id may follow a remedy as a trailing reference ("history:
  issue 1352"), never in place of one.
- **Contributor tier**: gates, `just` recipes, `nros dev` verbs, test
  output, and refusals only a contributor can trigger. Ids stay as they
  are.

Mechanics, to decide in the fix:
- an RFC-0117 amendment (or a short design note) stating the rule;
- a gate that scans the user-tier emitters (`warn(`, `cargo:warning`,
  `message(WARNING`, `nros_warn!` in the CLI's user verbs) for a message
  whose last clause is `(issue NNNN)` and has no remedy before it. It is a
  ratchet, since existing sites are many;
- a book anchor for each recurring remedy, for example "Declaring a node's
  parameters", which the knob reference and the contract chapter already
  half-cover.

## State on main (2026-11-07)

The warning above is gone. Issue 2270's fix (`0ede1760c2`) moved the report
to `nros_sizing_descriptor::defaults`, which says what to do ("declare each
node's `params:` in the `system.contract.yaml` beside `system.toml`") and
cites no id. That fixes the instance and leaves the rule unwritten. Other
user-reachable text still cites ids: for example, `build_param_slot.rs`'s
inbox refusal names issue 1352 mid-text, though it does give a remedy, and
nothing stops a new bare-id warning. The gate above is what would.
