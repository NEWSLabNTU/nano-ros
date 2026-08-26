---
id: 785
title: "`create_session` carries four of Humble's eight `rmw_init_options_t`
  fields, and one of the four it drops makes a GROUPED answer hollow"
status: resolved
type: bug
area: rmw
related: [phase-376, issue-0331, issue-0800, issue-0808]
---

## Problem

Upstream's `rmw_init_options_t` (Humble, `rmw/init_options.h`) has eight
fields. Our `create_session(locator, mode, domain_id, node_name, out)` carries
`domain_id` and nothing else from that list.

| upstream field | ours | verdict |
| --- | --- | --- |
| `domain_id` | `domain_id` | carried |
| `implementation_identifier` | `get_implementation_identifier` slot + `nros_rmw_descriptor_t` | answered elsewhere |
| `impl` | `rmw_session_t::backend_data` | answered elsewhere |
| `allocator` | — | declined ABI-wide, no allocator at this seam |
| `instance_id` | — | rcl-side process identity, not middleware behaviour |
| `security_options` | — | see below |
| `localhost_only` | — | **gap** |
| `enclave` | — | **gap, and it makes a grouping hollow** |

### `enclave` — we answer an upstream symbol whose payload we can never fill

`rmw_get_node_names_with_enclaves` is recorded as answered, GROUPED onto our
`get_node_names` slot. The grouping argument is sound in shape: upstream split
the two names only because appending to a fixed out-parameter list would break
its ABI, and a visitor has no such list, so the enclave is just a fourth
callback argument.

It is hollow in content. Nothing in this ABI ACCEPTS an enclave — there is no
field on `create_session`, no field on `rmw_node_t` — so
`rmw_node_visit_fn`'s `enclave` argument is structurally always NULL. We report
the symbol as answered and can only ever answer it with "no enclave".

That is not the same defect as a missing slot, and it is worse than one in a
particular way: the parity report counts it in the "answered" column.

### `localhost_only` — a real discovery-scope control

Upstream's `rmw_localhost_only_t` restricts discovery to the local host. On
cyclonedds that maps onto a real DDS setting, and it is a common deployment
knob (a test that must not see the rest of the lab network; an image on a
shared bus). We have no way to express it — the locator is the only
discovery-shaped input `create_session` takes, and for a domain-discovered
backend like cyclone the locator is unused.

### `security_options` — declined, with a reason that holds

`rmw_security_options_t` is `{rmw_security_enforcement_policy_t
enforce_security; char *security_root_path;}` — a DDS-SROS2 keystore path plus
an enforce/permissive switch. The path is a FILESYSTEM path and the mechanism
is a DDS security plugin; neither exists on the targets this ABI is for, and
the two backends that could honour it (cyclonedds hosted) are exactly the ones
where a caller can configure the participant out of band. Declined, and the
reason is about the target rather than about our convenience.

## Two stale claims fixed alongside this

Both places in `rmw_vtable.h` that enumerate the init options said
`rmw_init_options_t` carries "domain_id, enclave, security_options and
discovery_options".

* **`discovery_options` is not a Humble field at all.** It arrived in Iron.
  Our recorded contract (`docs/reference/rmw-implementation-contract.txt`) is
  Humble, so the list named a field the target distro does not have.
* The list omitted `instance_id`, `localhost_only`, `allocator`, `impl` and
  `implementation_identifier` — five of eight.

Third time in this campaign that an unchecked sentence about an upstream struct
turned out wrong (see issue 0777, twice). The fix is the same each time: read
the header in the distrobox.

## Fix

`localhost_only` and `enclave` both want the same thing — init-time context
that `create_session`'s flat argument list cannot grow without another ABI
break. Issue 0331 already proposes folding backend-shaped session config behind
the locator; whatever shape that takes should carry these two, and the
`enclave` grouping should stay recorded as hollow until it can be filled.

## Resolution, 2026-08-26

Three of the four asks are done. The fourth — actually CARRYING `localhost_only`
and `enclave` — could not be done here and, it turns out, was deferred to
nothing; that is the finding this issue ends on.

### The answered column stops over-counting, per SYMBOL

The complaint was that `rmw_get_node_names_with_enclaves` is recorded as
answered while nothing in this ABI can fill an enclave. Rather than special-case
that one symbol, the question is now asked of every contract symbol, using the
producer/consumer dimension issue 0800 added. `rmw-api-parity.py` reports:

```
  vtable     70
  ...
  Of the 70 contract symbol(s) in the `vtable` column, 34 are answered by a slot
  something writes or reads, and 36 by an INERT one (issue 0785).

## answered by an inert slot (36)
  ...
```

So the hollow grouping was not an isolated case. **36 of the 70 symbols in the
answered column rest on a slot nothing writes and nothing reads** — including
both halves of this grouping, because `get_node_names` is itself inert. The
enclave argument being structurally NULL is now the smaller of the two facts
about that row.

### The stale claims are corrected

`discovery_options` is gone from the surviving enumeration in `rmw_vtable.h` and
from the parity map's `rmw_init_options_init` detail. Both now say what the
eight Humble fields actually are and what happens to each: `domain_id` carried,
`implementation_identifier` / `impl` answered elsewhere, `allocator` and
`instance_id` declined, `security_options` declined on the target,
`localhost_only` and `enclave` gaps. The historical note recording the wrong
version stays, deliberately.

### The deferral was dangling — issue 0808

This issue's Fix said the two gaps should be carried by "whatever shape issue
0331 takes". **Issue 0331 is resolved**, and its own resolution says the
structural half — folding backend-shaped session config behind the locator — is
NOT done, deferring it to issue 0330 part 3. **Issue 0330 is also resolved**,
and its part 3 turned out to be about something else entirely (moving a
force-link anchor to `nros::force_link_backend!`). Neither did the fold.

So `rmw_vtable.h` was telling readers "see issue 0331" for work that issue
explicitly declined to do and that no open issue owned. Three separate needs —
the backend-shaped `mode`, `localhost_only`, `enclave` — all pointed at nothing.

Filed as **issue 0808**, which exists to be that home, and the two pointers in
`rmw_vtable.h` now name it. A deferral to a resolved issue is indistinguishable
from a deferral to a plan; that is the reusable part of this.

Same failure mode as the rest of this campaign, in a third medium: issue 0781
had a capability with two mechanisms and no test, issue 0800 had slots with no
producer and no consumer, and this one had a fix with no owner. Each read as
handled.
