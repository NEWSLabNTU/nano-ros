---
id: 1317
title: "A Zephyr C++ component fails to compile when its translation unit
  reaches component.hpp before anything includes <new>"
status: resolved
type: bug
area: api, cpp, zephyr
severity: high
resolved_in: "fix(#1317): component.hpp includes <new> on Zephyr instead of waiting for a consumer to"
related: [issue-0730]
---

## What happens

A Zephyr C++ image carrying a component stops at

```
packages/api/nros-cpp/include/nros/component.hpp:655:67: error: no matching
function for call to 'operator new(sizetype, void*&)'
```

which is `NROS_COMPONENT`'s `new (storage) Class(...)`.

## Why

Zephyr's minimal libcpp ships a stub `<new>` that declares `nothrow_t` but not
placement new, so `component.hpp` carries the standard non-allocating forms
itself. They sit behind `#ifdef ZEPHYR_SUBSYS_CPP_INCLUDE_NEW_`, which is
Zephyr's OWN include guard for that header: it is defined only after something
has already included `<new>`.

So whether the shim exists depends on the consumer's include order. A
translation unit that includes a node header which includes `component.hpp`
before anything drags `<new>` in compiles the shim away, and the factory macro
below it has no placement new.

Measured on Autoware Safety Island, four C++ component sources built for
native_sim: three of them reached `<new>` transitively and compiled;
`stop_mode_operator.cpp` did not and failed. Nothing in the failing file is
unusual, and nothing in this header can see the difference.

## Fix

`component.hpp` includes `<new>` itself under `__ZEPHYR__`, immediately before
the guarded block. The guard is then always defined on Zephyr, the shim always
compiles, and the factory always has placement new whatever the consumer
included first. Zephyr's stub has no placement new of its own, so there is
nothing to collide with.

Verified: the island's four-component native_sim image failed to compile
before this change and links after it.
