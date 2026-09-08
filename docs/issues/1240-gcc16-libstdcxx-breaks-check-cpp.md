---
id: 1240
title: "`just check cpp` cannot run on a GCC 16 host — libstdc++'s own headers fail under `-Werror` + the new `-Wtemplate-body`"
status: open
type: limitation
area: cpp, build, ci
severity: medium
found: 2026-09-09
related: [0872, 1187]
---

# The compile tier is unrunnable on this host, and the instruction assumes it is not

CLAUDE.md is unambiguous: **green CI locally BEFORE pushing** — run `just format`
and the tier your change earns, fix every failure, so the push passes remote CI
on the first try. On a host with GCC 16 that instruction cannot be followed for
anything that reaches `check-cpp`: the lane fails before it has compiled a line
of nano-ros code.

Measured 2026-09-09 on Arch, `gcc (GCC) 16.1.1 20260728`, running `just ci gate`
on a branch whose entire diff is one shell script.

## What fails

Two distinct failures inside `/usr/include/c++/16`, both raised while compiling
our headers but neither in our code:

```
/usr/include/c++/16/bits/stl_vector.h:2195:11: error: there are no arguments to
  '__throw_length_error' that depend on a template parameter, so a declaration
  of '__throw_length_error' must be available [-Wtemplate-body]

/usr/include/c++/16/sstream:1150:57: error: no type named 'pos_type' in
  'std::__cxx11::basic_stringstream<wchar_t>::traits_type'
  {aka 'struct std::char_traits<wchar_t>'}
```

`-Wtemplate-body` is new in GCC 16: it diagnoses two-phase-lookup problems in
template bodies that earlier releases only reported at instantiation. The lane
compiles with `-Werror`, so a warning in a SYSTEM header becomes an error — and
these are libstdc++'s own template bodies, not ours.

## The cascade, which is what makes the log misleading

Once `<vector>` and `<sstream>` have failed, `<memory>` is not usable either, so
`NROS_CPP_HAS_SHARED_PTR` never gets defined and the compiler reports:

```
packages/api/nros-cpp/include/nros/timer.hpp:68:28: error: 'shared_ptr' in
  namespace 'std' does not name a template type
  note: 'std::shared_ptr' is defined in header '<memory>'; this is probably
  fixable by adding '#include <memory>'
```

That note is wrong here, and it is the trap. `timer.hpp` DOES include `<memory>`
— twice, once under `NROS_CPP_STD` and once behind the `__has_include` probe
phase-417 W1.a added for issue 0112. Taking GCC's suggestion would add a third
copy of an include that is already there and fix nothing. **The `shared_ptr`
errors are downstream of the libstdc++ failure above them; read the FIRST error
in this lane, never the most specific-looking one.**

## Why nothing caught it

CI runs `ubuntu-22.04` (GCC 11), where neither diagnostic exists, so the lane is
green there and will stay green. `check-cpp` is in the compile tier, which the
`pre-push` hook deliberately excludes, so a contributor on a rolling distro
meets this only when they follow the instruction to run the tier locally — which
is the population most likely to be doing the right thing.

Same shape as issue 0872 one axis over: an arm that has never run to completion
in one environment, where each fix exposes the next gap. Distinct from issue
1187, which is `<string>` reaching an embedded FreeRTOS C++ image — that is our
own porting surface, this is a host toolchain we do not build against anywhere.

## What would resolve it

Not yet decided; the options differ in what they give up.

* Stop `-Werror` from applying to system headers in this lane
  (`-isystem` for the toolchain's own include dirs is the usual mechanism, and
  is arguably correct regardless — we do not own those warnings).
* Pin a supported host compiler for `check-cpp` the way issue 1117 pins a cross
  toolchain, and say so when the host's default is not one.
* Suppress `-Wtemplate-body` specifically, which is the narrowest fix and the
  one that goes stale the fastest.

The first is most likely right: a `-Werror` build has an opinion about ITS OWN
code, and letting it have one about libstdc++ is what broke here.

## Reproduce

```bash
just check cpp     # on a GCC >= 16 host
```

First error is in `/usr/include/c++/16/...`, not under `packages/`.
