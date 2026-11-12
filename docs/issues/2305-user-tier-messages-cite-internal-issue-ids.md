---
id: 2305
title: "User-tier messages cite internal issue ids (`(issue 1352)`) where they should say what to do; there is no policy separating them from contributor-tier refusals"
status: open
type: tech-debt
area: cli, ux, logging, build
related: [phase-537, rfc-0117, rfc-0116, 2270, phase-523, issue-2443, phase-552]
found: 2026-11-06
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md).** Found by the phase-537 W5 book session (PR #3187).
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

## Progress (2026-11-11)

**The policy exists; what was missing was its gates' reach.** RFC-0116 D2
(consolidated in RFC-0124 Principle 5) already rules that the user surface
"names the next action and never a phase, issue or RFC id", and
`check-user-surface-ids` enforces it, baseline zero, over the CLI's own
`cmd/` strings, the rendered help, `board list/show`, `doctor --report` and
the user book. phase-517 W5's `check-cmake-user-output-refs` covers what a
default CMake configure prints, as a ratchet. Neither read a build script's
warnings, and the CMake ratchet still recorded 60 sites. So no new tier rule
was written.

Done:

- **CMake.** Every default-level message (`STATUS`, `NOTICE`, `WARNING`,
  `AUTHOR_WARNING`, now `DEPRECATION` too) and custom-command `COMMENT` that
  cited an id was rewritten, and `.config/cmake-user-output-refs.json` is
  empty: the ratchet is now a zero baseline.
- **Build-script warnings.** `check-user-surface-ids` reads a build script's
  `cargo:warning` text (and text handed to a `warn()` / `.note()` helper).
  The six that cited an id were rewritten. A warning assembled away from its
  print is not seen; the gate's docstring says so.
- Each id moved into a `# History:` / `// History:` comment above its
  message, as D2 says for help text.

Not taken: the trailing `history: issue N` allowance proposed above. It would
loosen D2, which allows no id on the user surface; history moves to a comment
instead.

## Open question for the user

**Does a refusal (a CMake `FATAL_ERROR`, a build-script panic) name an id?**
Two rulings disagree:

- RFC-0116 D2: "every refusal … names the next action and never a phase,
  issue or RFC id".
- phase-517 W5 (`check-cmake-user-output-refs`'s docstring): `FATAL_ERROR`
  and `SEND_ERROR` are not counted, because "a build that stops is being
  diagnosed, and the issue that explains it is a fair thing to name there".

Measured 2026-11-11: about 100 `FATAL_ERROR` messages in `cmake/`, `zephyr/`
and the packages' CMake, and about 25 build-script panic texts, cite an id.
This change follows phase-517 W5 and leaves them, since choosing between the
two changes what a user sees at a failed build. A sweep of both was done and
reverted; it is mechanical once ruled (each id moves to a comment, and where
that leaves no action, the action is written, e.g. "Add TYPED to the call").

**Ruled 2026-11-12** ("Take your recommendation", option A, relayed by the
manager): "user-reachable output, build-stopping refusals included, names
what/where/how-to-fix and NO phase/issue/RFC id. Ids live in source comments
and `-v` detail." RFC-0116 D2 wins. `check-cmake-user-output-refs` now counts
`FATAL_ERROR` and `SEND_ERROR` too, and `check-user-surface-ids` reads
build-script panic texts. Both start as counted ratchets: 102 CMake sites and
17 panic ids, measured 2026-11-12. The sweep that lowers them is
[issue 2443](archived/2443-runtime-log-and-colcon-messages-cite-ids.md)'s.

The surfaces neither gate reads, runtime logs of a default image and the
colcon plugin's errors, are [issue 2443](archived/2443-runtime-log-and-colcon-messages-cite-ids.md).
