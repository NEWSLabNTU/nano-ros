---
id: 277
title: "nros_find_interfaces topo-last superset requires ONE closure per workspace — mixed msg-dep subsets miss or duplicate symbols"
status: resolved
type: friction
area: cmake
related: [0253, rfc-0057, phase-306]
resolved_in: "phase-306 W1 (per-package FFI crates); verified + gated 2026-07-26"
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

Phase-305 D3 landed: `nano_ros_auto_add_library` auto-wires the generated
interface libs — the manual topo-last `if(TARGET …__nano_ros_cpp)` blocks
are gone from the ASI workspace and in-tree examples. Remaining:
workspace-wide union closure (this issue's engineering half).

## Resolution (2026-07-26)

Resolved by phase-306 W1 — and verified rather than assumed. The engineering
half this issue was holding open (workspace-wide union closure) is **not
needed**: the per-package FFI-crate split removes the failure mode by
construction, so there is no closure to compute.

`NanoRosCodegenCore.cmake` already claimed this in a comment. The claim is now
backed by evidence from a real mixed-subset workspace, `examples/workspaces/cpp`
(the `metadata_cpp` fixture), which on posix configures in ONE pass:

- `talker_pkg` / `listener_pkg` — msg deps `{std_msgs}`
- `cpp_add_{server,client}_pkg`, `cpp_fib_{server,client}_pkg` — msg deps
  `{example_interfaces}`

Each package calls `nros_find_interfaces` from its own `package.xml`, so the
resolution sets are disjoint — exactly the shape described above. Observed:

1. **Per-package crates, deduped workspace-wide.** talker_pkg builds
   `nano_ros_cpp_ffi_{std_msgs,builtin_interfaces}`; cpp_add_server_pkg builds
   `nano_ros_cpp_ffi_{example_interfaces,action_msgs,unique_identifier_msgs}`.
   `builtin_interfaces` is built ONCE despite both subtrees needing it — no
   second copy appears under the other package.
2. **Zero duplicate exports.** Across every `libnano_ros_cpp_ffi_*.a` in the
   workspace, no `nros_cpp_*` symbol is defined by more than one archive. (The
   scan detects 90 duplicates when deliberately fed cargo's `deps/`-hashed copy
   of one archive, so it is measuring something.)
3. **Everything links.** All native entries — those linking the `std_msgs`
   subset and those linking the `example_interfaces` subset — build clean.

The `island_interfaces` union-shim package in the example repo is therefore
obsolete and can be deleted.

### Gate

Nothing asserted this property: the `metadata_cpp` and `local_msg_pkg` fixtures
were BUILT by `compile-check-fixtures.sh` but no test read them for symbol
overlap, so a regression would only surface as a mysterious downstream link
failure. Added
`packages/testing/nros-tests/tests/interface_subset_linkage.rs`:

- `mixed_subset_workspace_builds_per_package_ffi_crates` — asserts the fixture
  really contains archives owned by >= 2 packages, so the duplicate check below
  cannot pass vacuously.
- `interface_archives_carry_no_duplicate_exports` — `nm`s every archive and
  fails naming both owners if any `nros_cpp_*` symbol is defined twice.

### What stays true

RFC-0057 D3 (`nano_ros_auto_add_library` auto-wiring the generated interface
deps) remains the UX half and landed in phase-305. Between the two, a consumer
neither hand-picks an archive nor needs a shim package.
