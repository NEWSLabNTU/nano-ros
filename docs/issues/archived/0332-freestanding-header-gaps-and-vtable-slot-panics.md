---
id: 332
title: "Freestanding-header gaps the gate cannot catch: bridge.hpp includes <string>/<vector> ungated, check.h printf's from a public C header, and 21 vtable slots .expect() on the embedded path"
status: resolved
type: bug
severity: medium
area: core
related: [issue-0112]
---

## Finding (audit 2026-07-28, P2)

### 1. A new issue-0112 instance — and the gate structurally cannot see it

`packages/core/nros-cpp/include/nros/bridge.hpp:17-18` includes `<string>` and
`<vector>` **completely ungated** (and uses them as `SessionSpec` member types),
with no `NROS_CPP_STD` guard and no "hosted only" note. The 0112 rule is that
hosted includes gate on `NROS_CPP_STD`, never on `__STDC_HOSTED__` alone, because
a hosted compiler can still be run `-nostdinc++` against Zephyr's minimal libcpp.

Worse than the header itself: the `check-cpp` freestanding probe
(`justfile:1806-1829`) **does compile this header** but skips only
`rclcpp_compat.hpp` and **does not pass `-nostdinc++`** — so the probe cannot
detect the 0112 class at all. The gate that exists to prevent this has a blind
spot the size of the rule.

Fix: wrap the header body in `#ifdef NROS_CPP_STD` (as `std_compat.hpp` does) or
move the `std::`-typed spec structs behind that gate; **and** add a
`-nostdinc++` variant to the probe loop so the class becomes detectable.

Verified NOT findings: `guard_condition.hpp`, `timer.hpp`, `parameter.hpp`,
`std_compat.hpp`, `component_node.hpp` are all correctly `NROS_CPP_STD`-gated.
`rclcpp_compat.hpp:62-66` is fully ungated but is deliberately excluded from the
probe (phase 209) — by design, not a finding.

### 2. A public C header printf's unconditionally

`packages/core/nros-c/include/nros/check.h:32-36` — the default
`NROS_CHECK_LOG` macro `#include <stdio.h>` and calls `printf(...)` in a header
consumed by embedded no_std C nodes: no hosted gate, no log-level gate. Every
`NROS_CHECK`/`NROS_SOFTCHECK` failure prints, and every TU pulls in stdio unless
the user pre-defines the macro. (`result.hpp:144-151` at least conditionalises
its variant.)

Fix: default `NROS_CHECK_LOG` to the `nros-log` macro (or a no-op on
freestanding); keep the `printf` form behind `NROS_CPP_STD` / an explicit
`NROS_CHECK_STDIO` opt-in.

### 3. 21 vtable slots panic instead of erroring, and registration doesn't check

`packages/rmw/cffi/src/lib.rs` — 21 `.expect("rmw vtable: …")` calls
unwrap `Option<extern fn>` vtable slots on the embedded runtime path, including
the hot-path `drive_io`, `has_data`, `try_recv_raw`, `publish_raw` (lines 1220,
1302, 1370, 1437, 1494, 1523, 1535, 1637, 1914, 2030, 2055, 2072, 2254, 2261,
2398, 2422, 2465, 2479, 2505, 2527, 2641).

Meanwhile `nros_rmw_cffi_register_named:756` validates only the name and the NULL
pointer — it never checks that the mandatory slots are `Some`. A C backend
registered with an incomplete vtable therefore passes registration and panics
mid-spin on a no_std target, which is the worst place to discover it.

Fix: add a required-slot completeness check in `nros_rmw_cffi_register_named`
returning `NROS_RMW_RET_INVALID_ARGUMENT`, then downgrade the call sites to
`ok_or(TransportError::Unsupported)?` so a genuinely optional slot is a typed
error rather than a panic.

## Why grouped

All three are "the embedded/freestanding contract is asserted but not enforced",
all in `packages/core`, and the first one's fix includes repairing the gate that
should have caught it.

## Resolution (2026-07-28)

**Defect 1 — DONE.** `bridge.hpp`'s whole body is wrapped in `#ifdef
NROS_CPP_STD` (the multi-RMW bridge is hosted-only). Since the `-ffreestanding`
compile probe runs against the host's full libstdc++ and structurally cannot see
the 0112 class, added `scripts/check-cpp-freestanding-includes.sh` (wired into
`check-fast`): a source gate that flags any hosted STL include outside an
`NROS_CPP_STD` region in nros-cpp headers. Verified it catches the pre-fix
ungated bridge and passes the gated one. c61abe897.

**Defect 2 — DONE.** `check.h`'s default `NROS_CHECK_LOG` now gates the `printf`
form on `__STDC_HOSTED__` or an explicit `NROS_CHECK_STDIO`; freestanding
defaults to a `(void)`-cast no-op. Verified: a freestanding TU preprocesses to
zero `stdio` references; hosted still prints. c61abe897.

**Defect 3 — DONE (the bug); the refactor is deferred by design.**
`nros_rmw_cffi_register_named` now rejects an incomplete vtable at registration
(`first_missing_vtable_slot` → `NROS_RMW_RET_INVALID_ARGUMENT`), so "passes
registration and panics mid-spin on a no_std target" is fixed — the failure is
now loud and early. The required set is exactly the 20 slots the code already
`.expect()`s (the same set the working `STUB_VTABLE` fills). df66f7bff.

The issue also suggested downgrading the 21 `.expect()` call sites to
`ok_or(TransportError::Unsupported)?`. That is a *different* design — it would
support genuinely-partial backends (an optional slot as a typed error when
*used*). The call sites do not uniformly permit it (some return `bool`, not
`Result`), and it changes the backend contract. Deliberately NOT done: the
current contract is that a registered backend must be complete, now enforced at
registration. Partial-backend support can be filed separately if a real
consumer needs it.
