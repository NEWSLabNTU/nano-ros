---
id: 972
title: "ROS domain 0 is both a legal domain and the `unset` marker, so asking for domain 0 explicitly is silently overridden"
status: open
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

## Direction

The fix is to stop overloading the value. `Option<u32>` at the Rust boundary, or
a sentinel that is not a legal domain (`u32::MAX`), or an explicit
`has_domain_id` flag beside the field in the C ABI struct. Which one depends on
whether the C ABI can change compatibly — the field is part of a `repr(C)`
struct that RFC-0054 makes the headers' SSoT, so this is an ABI question first
and a Rust question second.

Note `CONFIG_NROS_CYCLONE_DOMAIN_ID` has the same shape one layer down and
CLAUDE.md already warns about it ("never pin it to a literal in confs — the
phase-180 split-brain silently ran every cyclone image on domain 0"). Whatever
spelling is chosen here should be checked against that one so the tree ends up
with one answer rather than two.

## Acceptance

* Requesting domain 0 explicitly, against a session configured for a non-zero
  domain, either honours 0 or fails loudly — not silently substitutes.
* "Unset" is representable without colliding with a legal domain value.
* A test covers the explicit-zero case; today nothing distinguishes it.
