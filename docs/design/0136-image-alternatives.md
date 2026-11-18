---
rfc: 0136
title: "Image alternatives: images that never run together may share an address"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-478]
supersedes: []
superseded-by: null
---

# RFC-0136 -- Image alternatives: images that never run together may share an address

**Status:** Draft (2026-11-18), for review. Requested through the manager on
2026-11-18 with the ruling that exclusivity is declared, never inferred.

## Summary

A `system.toml` may declare a set of images MUTUALLY EXCLUSIVE:
`[image_alternatives] <group> = ["img-a", "img-b", ...]`. Two images of one
group are the same host built different ways, so the cross-image collision
check (`image_net::check_collisions`, RFC-0109, D10b of RFC-0103) lets them
share a static address and a MAC. Nothing else does: exclusivity is only ever
DECLARED, never inferred from names, boards or `os_config` lists, and an
address shared by any two images that can run together is still refused.

## Motivation / problem

`check_collisions` refuses two images with one static address (or one MAC) on
one shared network, naming both. That is right for two hosts. It is wrong for
VARIANTS of one host: a bringup that builds the same controller four ways
(plain, runtime statistics, CTF to UART, CTF to a RAM ring) on the same TAP
network, all at `192.168.10.2`. Only one of them is ever booted. Today the
check refuses them, so such a project cannot state that address in a board
network file (issue 2679) and keeps it in a Zephyr `prj-*.conf` instead, where
nothing checks it at all.

The check cannot tell variants from hosts by itself. Image ids like
`fvp-tap` and `fvp-tap-trace` LOOK related, but a name rule would also excuse
`robot1` / `robot1-arm`, which do run together, and that failure is the silent
"identical IP, no delivery" trap the check exists for. So the project states
it.

## Design

### D1 -- Syntax: a top-level `[image_alternatives]` table

```toml
[image_alternatives]
fvp-tap = ["fvp-tap", "fvp-tap-stats", "fvp-tap-trace", "fvp-tap-trace-ring"]
```

Each key is a group name; each value lists two or more image ids. Membership
is symmetric: no member is the "base".

Why a group table and not a per-image `variant_of = "<image>"`:

- **One place states the whole set.** A reviewer reads one line to know which
  images may share; with `variant_of` the set is spread over N blocks and is
  the transitive closure of edges, which nobody reads off a diff.
- **No primary.** `variant_of` forces one image to be the base, which is a
  fact the project does not have (no variant is more real than another), and
  invites chains (`a -> b -> c`) and cycles that need their own rules.
- **Stays out of the image fold.** An `[image.<id>]` key is folded over
  `[image_defaults]` (RFC-0065 D5.1); a `variant_of` there would be accepted
  in `[image_defaults]` too, meaning "every image is a variant of X". A
  top-level table cannot be folded.
- **Named like its neighbour.** `image_` prefix as `[image_defaults]`, so it
  is not read as a sub-table of `[image.<id>]` and does not collide with a
  future image id `alternatives`.

### D2 -- Validation (at parse, beside `validate_default_images`)

`SystemToml::validate_image_alternatives`, refusing:

| Case | Error |
| --- | --- |
| unknown image | ``[image_alternatives] `<g>` names image `<id>`, which no `[image.<id>]` declares`` |
| image in two groups | ``image `<id>` is in two `[image_alternatives]` groups, `<g1>` and `<g2>`; an image has one set of alternatives -- merge the groups`` |
| fewer than two members (incl. empty) | ``[image_alternatives] `<g>` lists <n> image(s); a group needs two or more`` |
| same image twice in a group | ``[image_alternatives] `<g>` lists image `<id>` twice`` |

Groups are disjoint (the second row), so "in one group" is an equivalence
relation and `check_collisions` never has to reason about overlapping sets
(`a` with `b`, `a` with `c`, but `b` with `c` running together).

The single-package leaf schema (`leaf_system`) has one image, so it does not
gain the table; it stays an unknown key there.

### D3 -- `check_collisions`

It takes the groups (`&ImageAlternatives`, id -> group) beside the images.
Per `(network, address)` and per `(network, mac)` it keeps the FIRST holder;
a later image with the same key passes only when it and the first holder are
in one group. Since every holder is compared with the first, a set of images
sharing one address passes exactly when all of them are in one group.

The refusal keeps today's text and adds the declared way out:

```text
images `fvp-tap` (eth0) and `robot` (eth0) both take 192.168.10.2 on network
"tap" -- two hosts on one network cannot share an address; change one `host` /
`address`, or, if the two never run together, list both in one
`[image_alternatives]` group
```

When the two ARE in different groups, the hint names them instead: ``... are in
different `[image_alternatives]` groups (`g1`, `g2`)``.

Shared MACs follow the same rule: a variant is the same NIC. A derived MAC
differs per image id (RFC-0109), so only a stated `mac` can meet.

Nothing else is relaxed: subnet, gateway and network-definition checks are
per image and unchanged.

### D4 -- Running: never two members at once

Building is unaffected: `nros build` of two members (both in
`default_images`, say) is fine, building is not running.

Every nano-ros surface that STARTS more than one image in one invocation
refuses two members of one group:

```text
images `fvp-tap` and `fvp-tap-trace` are alternatives (`[image_alternatives]
fvp-tap`) and share 192.168.10.2; start one of them
```

Today `nros run` starts one image per invocation, so it has nothing to check.
The rule binds the multi-image runners as they land (the RunPlan test harness,
phase-528 W5; any compose / launch of several images). Two separate
`nros run` invocations cannot be seen from either, and no run-time lock is
proposed (see Open questions).

### D5 -- Tests

Unit tests in `orchestration/image_net.rs` and `cargo_metadata_schema.rs`:

- two images, one address, one group: pass;
- the same pair, no group: refused, message carries the
  `[image_alternatives]` hint;
- three images on one address, two grouped and one not, in each insertion
  order: refused, naming the ungrouped one;
- two images in two different groups on one address: refused, naming both
  groups;
- a shared stated MAC inside a group: pass; across groups: refused;
- validation: unknown image, image in two groups, group of one, empty group,
  duplicate member -- each with its exact text;
- leaf `system.toml` with `[image_alternatives]`: refused as unknown key.

One `resolve_system` test over a bringup with four TAP variants on one board
network file, mirroring the motivating case.

### D6 -- Docs

- `book/src/reference/system-toml.md`: an `[image_alternatives]` section, and
  the "Refused, naming the image" paragraph of "Board network file" gains
  "unless both are in one `[image_alternatives]` group".
- `book/src/guide/build/configure-an-image.md`: one paragraph, "variants of
  one host", with the example of D1.
- A `changelog.d` feat fragment when it is built (none for this RFC).

## Alternatives considered

- **`variant_of` per image.** Lost on D1's grounds.
- **Infer from names or identical `os_config` / board.** Refused by ruling:
  exclusivity is a fact about how the project is run, which no build input
  carries. A wrong guess reproduces the silent no-delivery bug.
- **A per-image opt-out (`allow_shared_address = true`).** Says nothing about
  WITH WHOM, so it would excuse a collision with an image that does run
  alongside.
- **Drop the check for statically addressed Zephyr images.** Loses the check
  for every project to serve one.

## Open questions

1. Should `nros run` keep a per-workspace marker of a running member, so a
   second `nros run` of another member warns? Proposed: no, until a user
   reports running two by mistake.
2. Should `nros image list` / `show` (RFC-0133) print an image's group?
   Proposed: yes, as a column, when RFC-0133 lands.

## Changelog

- 2026-11 -- created (issue 2690).
