---
id: 277
title: "nros_find_interfaces topo-last superset requires ONE closure per workspace — mixed msg-dep subsets miss or duplicate symbols"
status: open
type: friction
area: cmake
related: [0253, rfc-0057]
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 12)

With the 0253 mitigation (per-call topo-last superset FFI crate,
`NO_FFI_CRATE` on the rest), a workspace whose node packages call
`nros_find_interfaces` with DIFFERENT msg-dep subsets gets either missing
or duplicated symbols — the superset is computed per call, not per
workspace. The example repo works around it with a `src/island_interfaces`
shim package (forced first SUBDIR) that resolves the UNION closure once;
later interface calls no-op idempotently.

Every multi-node workspace will rediscover this. Either:
- compute the union closure workspace-wide (defer FFI-crate emission to the
  end of the configure pass), or
- detect the mixed-subset case and FATAL_ERROR with the shim-package recipe.

## Partial mitigation (2026-07-25)

`nros_find_interfaces` now tracks resolved packages workspace-wide
(GLOBAL property) and emits a loud `message(WARNING …)` when a later CPP
call introduces packages NOT covered by an earlier call — naming the
union-shim recipe. Subset calls (the `island_interfaces` pattern) stay
silent; verified against the autoware-safety-island workspace configure.
Remaining work: compute the union closure workspace-wide so the shim
package becomes unnecessary.

RFC-0057 D3 covers the UX half: `nano_ros_auto_add_library` auto-wires
the generated interface deps so consumers never hand-pick the topo-last
archive. The workspace-wide union closure stays this issue's engineering
half.
