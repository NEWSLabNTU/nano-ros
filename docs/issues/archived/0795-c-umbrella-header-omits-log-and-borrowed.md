---
id: 795
title: "`nros/nros.h` omits `log.h` and `borrowed.h`, so two whole C capability
  surfaces are unreachable from the umbrella header — and invisible to every
  tool that reads it"
status: resolved
type: bug
area: api
related: [rfc-0033, phase-379, issue-0589]
---

## Problem

`packages/api/nros-c/include/nros/nros.h` is the umbrella header — what a C user
includes, and what `scripts/api-parity.py` parses to extract the C surface. It
does not include two of its siblings:

**`nros/log.h`** — the whole C logging surface: `nros_log_emit`,
`nros_log_emit_fmt`, `nros_log_init`, `nros_log_default_logger`,
`nros_logger_t`, `nros_log_severity_t`, and six `NROS_LOG_*` macros. The C++
umbrella `nros/nros.hpp` *does* include `nros/log.hpp`, so C++ users get the
equivalent and C users do not.

**`nros/borrowed.h`** — the entire RFC-0033 C zero-copy reader:
`nros_cdr_borrow_string`, `nros_cdr_borrow_bytes`, ten
`nros_cdr_borrow_le_slice_*`, `nros_borrowed_str_t`, `nros_borrowed_bytes_t`, the
`nros_le_slice_view_*_t` family. Only generated message headers include it, so a
C user who includes the umbrella has no zero-copy read path.

## Why it matters twice over

**For users**, the C API is narrower than it is. A C author reaching for a
logger through `<nros/nros.h>` finds nothing and writes `printf`. That is not
harmless: CLAUDE.md's issue-0589 rule exists because reaching for the obvious
stdio call is fatal on Zephyr native_sim, and the whole point of `nros_log` is to
be the thing found first.

**For tooling**, it makes the C lane read as missing capabilities it has. Phase
379's `log` stage sees one C row where there are seven, and its `serde` stage
sees no C zero-copy reader at all. Several `gap` verdicts in
`docs/reference/api-parity-ledger/log.json` and `serde.json` say so explicitly
rather than claiming C lacks the capability — but a reader who trusts the
correlator without those notes would conclude the wrong thing.

That second half is the more expensive one: the campaign's premise is that the
umbrella header IS the surface. Where that is false, every downstream number is
wrong in the same direction.

## Fixed 2026-08-25

Two includes added to `nros/nros.h`, each with the reason inline. Verified by
compiling `nros_log_default_logger()` through the umbrella header (it did not
resolve before), and by `just check c`.

The correlator immediately surfaced **33 C declarations that had been invisible
to it** — the whole `nros_cdr_borrow_*` / `nros_le_slice_view_*` family and the
five logging entry points — and `just check api-parity` failed until they were
classified, which is the loop working as intended. They are now `extension` rows
in `serde.json`, `types.json`, `log.json` and `other.json`.

`main.h` was left out: its seven symbols are board entry points
(`nros_board_freertos_run_tiers`, …), platform-specific and not part of a
portable surface. That is a judgement, so it is now written in `nros.h` rather
than inferred from absence.

## The audit nobody had run

`nros.h` includes 18 of its 30 siblings. Of the twelve it omits, the generated
and internal ones are reached transitively or are not API
(`nros_generated.h` via `types.h`, `visibility.h`, the two
`nros_config_generated*.h`, `nros.h` itself), and three carry public
declarations:

| header | `NROS_PUBLIC` symbols | verdict |
| --- | ---: | --- |
| `log.h` | the logging surface | **defect** — `nros.hpp` includes its C++ twin |
| `borrowed.h` | the RFC-0033 zero-copy reader | **defect** |
| `main.h` | 7 | plausibly deliberate — board entry points (`nros_board_freertos_run_tiers`, …), platform-specific and not part of a portable surface. But undocumented, so it reads the same as the two above. |

`bridge.h`, `component.h` and `sched_context.h` declare no `NROS_PUBLIC` symbols
and may be macro-only; they are worth a look but are not obviously wrong.

The point of the table is that this was found twice by two independent stages
and the set had never been audited. Whatever the answer per header, it should be
written in `nros.h` rather than inferred.

## Related, same shape, different header

`packages/api/nros-cpp/include/nros/component_node.hpp` is not included by
`nros/nros.hpp` either. `nros::ComponentNode` carries rclcpp's exact
`declare_parameter<T>` / `get_parameter<T>` / `has_parameter`, so through the
umbrella a ported C++ node gets `nros::Node` — which has no parameter method at
all — plus a standalone `ParameterServer<Cap>` the node cannot see. Recorded in
issue 0793.

## Evidence

* `grep -n include packages/api/nros-c/include/nros/nros.h` — no `log.h`, no
  `borrowed.h`.
* `packages/api/nros-cpp/include/nros/nros.hpp` — includes `nros/log.hpp`.
* `scripts/api-parity.py --topic log`, `--topic serde`, and the rows in
  `docs/reference/api-parity-ledger/{log,serde}.json` that name the omission.
