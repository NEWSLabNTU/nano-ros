---
id: 972
title: "ROS domain 0 is both a legal domain and the `unset` marker, so asking for domain 0 explicitly is silently overridden"
status: resolved
type: bug
area: api-c, rmw
related: [issue-0820, issue-0801, issue-0161]
---

## The code

`packages/api/nros-c/src/node.rs:739`:

```rust
let support_domain = support_mut.domain_id as u32;
let session = support_mut.get_session_mut()?;
let domain_id = if support_domain != 0 {
    support_domain
} else {
    session.domain_id()
};
```

The comment directly above states the ambiguity without resolving it:
*"`support.domain_id` is resolved from the C ABI argument, where 0 means
'unset' and resolves to 0"*.

## Why it is a defect and not a convention

`0` is **the default ROS domain** — the one a user gets by not setting
`ROS_DOMAIN_ID` at all, and therefore the most commonly intended value. So the
branch reads:

| caller meant | `support_domain` | what happens |
| --- | --- | --- |
| "I did not specify" | 0 | falls back to the session's domain — correct |
| "I want domain 0" | 0 | **falls back to the session's domain — wrong** |

The two are indistinguishable at this point, and the second is silent: no error,
no warning, and the entity simply lands on a different domain than asked for.
When the session was configured for a non-zero domain — which is exactly the
configuration `CONFIG_NROS_DOMAIN_ID` exists to produce — an explicit
`domain_id = 0` cannot be honoured at all.

The failure mode is the one issue 0801 already records for a neighbouring cause:
discovery never matches, and nothing reports an error, *"because the domain is
just the first element of the key"*.

## Where it came from

Extracted from [[issue-0820]], which hit it while chasing a museum binary that
published on domain 1. That investigation named this sentinel as a suspect,
then cleared it — the museum binary was a missing rebuild edge, not this — and
recorded that the ambiguity "IS real code and may deserve its own issue". This
is that issue. **Nothing here is evidence of a failure in the field yet**; it is
a latent ambiguity found by reading, and it should be reproduced before it is
fixed.

## What it actually was — worse than "silently overridden"

The sentinel this issue asked for ALREADY EXISTS. Issue #227 added
`NROS_DOMAIN_ID_EXPLICIT_ZERO = 255` and `baked_domain_from_c_abi`:

    0   -> None       (unset — defer to the next rung)
    255 -> Some(0)    (explicit domain 0)
    n   -> Some(n)

`nros_support_init` decodes correctly with it. `nros_support_t.domain_id` then
stores the byte EXACTLY AS PASSED — it is an encoding, not a domain — and the
two entity-resolution sites in `node.rs` read that raw byte and compared it
against 0, skipping the decoder entirely.

So the real behaviour was not "explicit zero gets overridden". It was:

    caller passes NROS_DOMAIN_ID_EXPLICIT_ZERO (255)
      -> 255 != 0, so 255 is used AS THE DOMAIN

255 is not a legal ROS domain at all — they cap at 232. A caller asking for the
default domain got an impossible one, silently, because a domain is just the
first element of the discovery key (issue 0801). The `0` half was correct and is
preserved.

## Fix

One decoder, `resolve_domain_from_c_abi(raw, session_domain)`, used at both
sites. It was the same mistake in two places and a third would have made it
three, so the decode lives in one function rather than being repeated.

## The tests did not run, which is how the real problem surfaced

The first run of the new test PASSED against the BROKEN code. It was not being
compiled: `nros-c`'s `mod node` sits inside `rmw_modules!`, which expands to
`#[cfg(feature = "rmw-cffi")]`, and `just test-unit` runs
`cargo nextest --workspace`, which activates no features — its own comment says
so. `check::workspace-features` does compile it with `rmw-cffi`, but only to
CLIPPY it: lint, never execute.

**Fifty-two unit tests in that file had therefore never run in any lane**,
including the phase-379 W4 reference-invalidation set that exists to pin a
deliberate behavioural change.

That is the class CLAUDE.md names — "a target behind `required-features` that no
recipe enables is the same lie one level up […] cargo skips it SILENTLY, so it
reads as coverage" — here a MODULE behind a feature rather than a target behind
`required-features`, with identical effect.

`check::required-features-tests` now runs `-p nros-c --features rmw-cffi --lib`;
that lane already exists for exactly this shape, including issue 0779's file-cfg
half. All 83 tests execute.

Verified non-vacuous: with the pre-fix logic restored,
`explicit_zero_resolves_to_domain_zero_not_255` fails with its own message. A
test that has never failed is not known to work — and this one had already
demonstrated that by passing against broken code.

## Still worth doing, not done here

`CONFIG_NROS_CYCLONE_DOMAIN_ID` has the same overloading one layer down
(CLAUDE.md: "the phase-180 split-brain silently ran every cyclone image on
domain 0"). Different mechanism, different owner; this fix does not touch it, so
the tree still has two answers to "how do you say unset".

## Acceptance

* ~~Requesting domain 0 explicitly […] honours 0 or fails loudly.~~ Met: it
  resolves to 0. It had been resolving to 255, an illegal domain.
* ~~"Unset" is representable without colliding with a legal domain value.~~ Met
  — it already was, via #227's sentinel; the defect was not decoding it.
* ~~A test covers the explicit-zero case.~~ Met, and it RUNS: the lane gap that
  made the first version of this test vacuous is closed too.
