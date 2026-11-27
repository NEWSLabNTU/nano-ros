---
id: 3016
title: "`just check cpp` fails under clang, and no merge-gating event runs it at all"
status: resolved
type: bug
severity: low
area: [cpp, tests, ci]
related: [issue-3003]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "fix(cpp, tests): check cpp passes under clang, and the merge queue runs it with both compilers"
---

## What happened

`just check cpp` compiled its C++ tests with a hard-coded `c++`, which is
gcc on this host and on every CI runner. With clang as `c++`, three
lanes failed:

- **cpp-runtime / cpp-runtime-scoped.** Test stubs in anonymous namespaces
  declared message-type constants (`TYPE_NAME`, `TYPE_HASH`,
  `SERIALIZED_SIZE_MAX`) that nothing in the TU reads. Under `-Wall
  -Werror`, clang's `-Wunused-const-variable` refuses them; gcc does not
  warn. Affected: `client_take_response_runtime.cpp`,
  `ament_port_surface_runtime.cpp` and `service_request_header_runtime.cpp`.
- **cpp-headers.** `one_node_type_ours_only_names.cpp`'s pre-rename
  NEGATIVE CONTROL asserted that
  `create_publisher<M>("chatter", ThenPortedQoS(10))` binds the by-value
  overload. Under ISO the call is AMBIGUOUS: argument 1 favours ours
  (standard vs user-defined conversion), argument 2 favours upstream
  (identity vs derived-to-base), and neither candidate is better on every
  argument. Clang rejects it. Gcc accepts it through its worst-conversion
  extension (a pedwarn) and binds ours. Checked against the real signatures
  at the rename: the reconstruction is faithful, so the defect the rename
  prevented was itself compiler-dependent. It was a silent wrong binding on
  gcc and a hard error on clang.

**And `check cpp` gated nothing before merge.** It is in the `build::`
gate list, which only `check-build` runs, on schedule and dispatch. No
pull-request or merge-queue job compiled a C++ test with either compiler.
Only `cpp-ffi-tests` and `cpp-heap-free-image-link` ran elsewhere. A
regression in these lanes, including the expected-failure probes (for
example issue 3003's move probe), surfaced the next night at best.

A blanket clang sweep of `tests/compile/` at `-Wall -Wextra -Werror`
flags 29 TUs. The lanes' own flags are narrower: most TUs are compiled
`-fsyntax-only` without `-Werror`, and some are expected-failure probes.
So only what the lanes actually reject was changed.

## Fix

- The three recipes take the compiler from `NROS_CHECK_CXX`, defaulting to
  `c++`. All 102 compiler invocations were converted.
- Stub constants that clang proves unread are deleted, not suppressed. Gcc
  still compiles every path, so a constant a path does need would fail the
  build.
- The negative control states the true, compiler-dependent fact:
  - on every compiler, the explicit-QoS call does not cleanly reach
    UPSTREAM (a SFINAE detector);
  - on gcc, a direct `decltype` still asserts it binds OURS;
  - on clang, the detector asserts it is not callable.
  Measured on gcc 11.4 and clang 14, at C++14 and C++17: in a SFINAE context
  BOTH compilers report the call not callable. Gcc's extension applies only
  outside SFINAE, which is why its branch keeps the direct `decltype`. The
  integer-depth control holds on both and stays unconditional.
- `gate.yml`'s `check-test-unit` job (merge group and dispatch) runs the
  three lanes with `c++` and then with `clang++`, one step per lane and
  compiler, so one red cannot hide the rest. Measured warm: 92 s per
  compiler. The three recipes leave `.config/ungated-gates.txt`.
- `check-cpp-lane-compiler` (fast line, self-testing) fails a literal `c++`,
  `g++` or `clang++` compile in any recipe that binds `_cxx`. It also fails a
  workflow running a recipe under `NROS_CHECK_CXX` that never binds `_cxx`.
  Without it, a newly added `c++ ...` step would be compiled by gcc in the
  clang pass too, and that pass would read green.

## Acceptance

- `just check cpp-runtime`, `cpp-runtime-scoped` and `cpp-headers` pass with
  `NROS_CHECK_CXX=clang++`, and `just check cpp` passes with gcc.
- A merge-queue job runs them under both compilers, and
  `check-lane-contracts` holds for it: the job builds every artifact the
  recipes resolve.
