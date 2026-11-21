# Phase 586 -- Image alternatives: variants of one host may share an address

**Status (2026-11-22). DONE: W1-W4 landed in one PR; D4 has nothing to bind
yet.** Implements [RFC-0136](../design/0136-image-alternatives.md) (approved
2026-11-18) for [issue 2690](../issues/archived/2690-image-alternatives.md),
homed as [phase-478](phase-478-island-developer-experience-findings.md) I32.

## Why

The safety island builds its FVP TAP host four ways (`fvp-tap`,
`fvp-tap-stats`, `fvp-tap-trace`, `fvp-tap-trace-ring`), all at
`192.168.10.2`, and boots one. `image_net::check_collisions` refused the
shared address, so the island could not name one board network file
(`net-fvp-tap.toml`) from the four images and kept the address in a Zephyr
`prj-*.conf`, where nothing checks it. The project must DECLARE which images
never run together; no build input says so (RFC-0136, ruling 2026-11-18).

## Work items

### W1 -- the table and its validation (RFC-0136 D1, D2)

- **Status:** DONE (2026-11-22)
- **What:** `SystemToml.image_alternatives`, a top-level
  `[image_alternatives] <group> = [ids]`, and
  `SystemToml::validate_image_alternatives`, called by the planner beside
  `validate_default_images`, by `nros build` before the collision check, and
  by `image_net::resolve_system` itself. A single-package leaf refuses the
  table by name (`leaf_system::from_system_toml`).
- **Acceptance:** each D2 refusal (unknown image, image in two groups, group
  of one, empty group, duplicate member) with its exact text; a leaf
  carrying the table is refused.

### W2 -- the collision check (RFC-0136 D3)

- **Status:** DONE (2026-11-22)
- **What:** `check_collisions(images, alternatives)` keeps each
  `(network, address)` and `(network, mac)` key's first holder and passes a
  later holder only when both are in one group. The refusal adds the way
  out: "list both in one `[image_alternatives]` group", or, when the two are
  in different groups, names both groups.
- **Acceptance:** one group shares an address and a stated MAC; the same pair
  with no group, one grouped and one not, or in two groups is refused with the
  matching hint; three images on one address with one ungrouped are refused
  naming it in all six insertion orders; one `resolve_system` test over four
  TAP variants naming one board network file on `fvp-aemv8r-smp`, refused
  without the group, resolved at `192.168.10.2` with it, and refused again
  for a fifth image outside the group.

### W3 -- docs (RFC-0136 D6)

- **Status:** DONE (2026-11-22)
- **What:** `book/src/reference/system-toml.md` gains the
  `[image_alternatives]` section and the "unless both are in one group"
  clause in "Board network file"; `book/src/guide/build/configure-an-image.md`
  gains the "variants of one host" step; a `changelog.d` feat fragment.
- **Acceptance:** the book builds; the pages name the table and its refusals.

### W4 -- the consumer

- **Status:** DONE (2026-11-22)
- **What:** the island's four `fvp-tap*` images, with one
  `[image_alternatives]` line, name `net-fvp-tap.toml` (192.168.10.2 on
  network `tap`) and configure.
- **Acceptance:** `nros build` of each of the four configures with the board
  network file and renders `CONFIG_NET_CONFIG_MY_IPV4_ADDR="192.168.10.2"`.
  The island adopts it in its own repository.

### D4 -- running (no work today)

`nros run` starts one image per invocation, so it has nothing to refuse. Any
runner that starts several images in one invocation (a multi-image RunPlan
harness, a compose or launch of several images) refuses two members of one
group when it lands; that rule is RFC-0136 D4's and binds it then.
