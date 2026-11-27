---
id: 3016
title: "`just check cpp` fails under clang: a C++ runtime test's unused type constants trip -Werror"
status: open
type: bug
severity: low
area: [cpp, tests]
found: 2026-11-28
---

## What happens

`just check cpp` compiles its C++ runtime tests with `c++ ... -Wall -Wextra
-Werror`. With clang as `c++` (measured: clang++ first on `PATH`), the lane
stops at `client_take_response_runtime.cpp`:

```
client_take_response_runtime.cpp:40:38: error: unused variable 'TYPE_NAME' [-Werror,-Wunused-const-variable]
client_take_response_runtime.cpp:58:38: error: unused variable 'TYPE_NAME' [-Werror,-Wunused-const-variable]
client_take_response_runtime.cpp:60:38: error: unused variable 'TYPE_HASH' [-Werror,-Wunused-const-variable]
```

The test's stub `Request` / `Response` structs declare
`static constexpr const char* TYPE_NAME` / `TYPE_HASH`. Nothing in that TU
reads them, and clang's `-Wunused-const-variable`, which `-Wall` enables,
reports a static constexpr member that is never used. GCC does not, so the
lane passes on a host whose `c++` is GCC, which is this one and CI's.

## Scope (not yet swept)

The clang run stops at the first failing TU, so other runtime tests later in
the lane may carry the same stub pattern. Sweep with clang as `c++` through
the whole lane before fixing. Grep the stub structs for
`static constexpr const char\* TYPE_` across
`packages/api/nros-cpp/tests/compile/`.

## Fix direction

Fix the class, not the one file. Either read the constants (if the
type-support path is meant to consume them, the test should exercise that),
or mark them `[[maybe_unused]]`, which is C++17 and so allowed in these
`-std=c++17` TUs. Do not drop `-Werror` for clang.

## Acceptance

`just check cpp` passes with clang++ as `c++`, and still passes with GCC.
