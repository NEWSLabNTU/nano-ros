---
id: 818
title: "`--check` green is not evidence of C++ API coverage: the parity extractor
  compiles one TU that never reaches `component_node.hpp` and never defines
  `NROS_CPP_STD`, so two whole families produce zero ledger rows"
status: open
type: bug
area: api, tooling
related: [phase-379, issue-0788]
---

## Problem

`scripts/api-parity.py:131` extracts our C++ surface from exactly one
translation unit:

```python
def ours_cpp(tmpdir):
    return extract_cxx.extract(
        '#include "nros/nros.hpp"\n',
        "c++", extract_cxx.nros_cpp_include_args(), {"nros"}, tmpdir)
```

Two families never reach the compiler through that door.

**1. `ComponentNode` — not included at all.** `nros/nros.hpp` includes
`node.hpp`, `publisher.hpp`, `subscription.hpp`, `service.hpp`, `client.hpp`,
the action and polling headers … and never `component_node.hpp`. Measured:
**zero** ledger rows repo-wide contain `ComponentNode`, while
`component_node.hpp` holds roughly half the C++ `create_timer` call sites plus
the `NROS_COMPONENT_*` macro.

**2. Everything behind `NROS_CPP_STD` — included but compiled out.**
`nros.hpp:186` reads

```c
#ifdef NROS_CPP_STD
#include "nros/std_compat.hpp"
#endif
```

and the extractor passes no `-DNROS_CPP_STD`. So `std_compat.hpp`'s free
functions are invisible.

## Why it matters

**A green `--check` is being read as "the C++ surface is accounted for". It is
not** — it is silent about every symbol in those two families, and silence is
indistinguishable from agreement in this tool.

That is not hypothetical; it produced two wrong ledger rows in one day:

* Phase-379 W5 group A renamed `nros::make_publisher` → `create_publisher` on a
  ledger row that recorded no collision. `nros::create_publisher` **already
  existed** in `std_compat.hpp`. It happens to form a legal overload set — proved
  with a compile probe — but the tool did not know it was there, and
  `--suggest-renames` scored it as unclaimed.
* `cpp:create_timer`'s row claimed "the fix is ADDING a free function".
  `nros::create_timer` already exists at `std_compat.hpp:61`, along with
  `create_timer_oneshot`.

The same blind spot means the `create_timer` → `create_wall_timer` decision
cannot be verified through the gate: half its sites are in a header the tool
never opens.

## Second-order: no gate covers the pairing either

`just check-cpp` compiles each header **standalone and without**
`NROS_CPP_STD`. So the configuration where `std_compat.hpp`'s declarations
coexist with the rest of the API is exercised by **neither** the parity tool nor
the C++ lane. Group A's overload set is correct today by inspection and by a
throwaway probe, not by anything that will still be running next month.

## Evidence

```sh
grep -n 'component_node\|std_compat' packages/api/nros-cpp/include/nros/nros.hpp
#   187:#include "nros/std_compat.hpp"      <- guarded, and only this one
sed -n '185,188p' packages/api/nros-cpp/include/nros/nros.hpp   # the #ifdef
grep -rn 'NROS_CPP_STD' scripts/api_parity/extract_cxx.py       # nothing
python3 - <<'PY'
import json,glob
print(sum(1 for f in glob.glob('docs/reference/api-parity-ledger/*.json')
           for k in json.load(open(f)) if 'ComponentNode' in k))   # 0
PY
```

## Direction

Three separable moves; none decided here.

1. **Extract more than one TU, or extract the header set rather than the
   umbrella.** The umbrella is a curated convenience header — using it as the
   definition of "our C++ API" silently makes any header it omits non-API.
   Whichever is chosen, `ComponentNode` has to land somewhere: it is a
   user-facing type with a public macro.
2. **Extract twice, with and without `NROS_CPP_STD`**, and mark rows that exist
   only in the `std` flavour. RFC-0036's flavour vocabulary already exists for
   this; the parity tool does not use it.
3. **Gate the pairing.** A `tests/compile/` TU that includes `nros.hpp` *with*
   `NROS_CPP_STD` would have caught the `create_publisher` overload question at
   the moment it was created rather than by inspection afterwards.

Until at least (1) lands, treat "`--check` is green" as a statement about the
headers `nros.hpp` reaches, and say so when citing it.
