---
id: 662
title: "The C/C++ metadata probe runs before the workspace's own interface packages are built, so every component that `find_package`es one is unprobeable"
status: open
type: bug
severity: medium
area: cli, build
related: [issue-0641, phase-308, phase-313, phase-367]
---

## Symptom

Every C/C++ component in `examples/workspaces/features` — **16 of them** — has
no source-metadata sidecar and falls back to the SystemModel's entity lower
bound. `nros sync --verbose` says so, once per component, and nothing else
notices:

```
sync: source metadata — no producer for cpp_qos_talker_pkg::cpp_qos_talker
      (probe project configure failed: …)
```

## Cause — an ordering constraint, not a bug in the probe

The probe project configures with `CMAKE_PREFIX_PATH` set to the nano-ros
checkout and nothing else. The components do:

```cmake
find_package(custom_msgs REQUIRED)
```

`custom_msgs` is a WORKSPACE-LOCAL interface package
(`features/src/custom_msgs`). It is built by the workspace's own CMake build —
which runs **after** `nros sync`, because sync is what generates the inputs that
build consumes. At sync time there is no `custom_msgsConfig.cmake` anywhere and
no install prefix in the workspace, so `find_package` cannot succeed.

Verified rather than assumed: `find … -name 'custom_msgs*Config.cmake'` returns
nothing, and the workspace has no `install/` prefix.

So these components are unprobeable **by construction at sync time**, not
because anything is malformed.

## What was fixed here (phase-367 W4)

The failure used to be all-or-nothing: one component's `find_package` error
aborted the whole batch configure, so *every* component in the workspace lost
its sidecar. That contradicted this driver's own contract, which it states for
the BUILD step a few lines below:

> ONE unprobeable component degrades to the sidecar-less path by NAME rather
> than taking the whole workspace with it.

It held for builds (there is a per-target fallback) and not for configures,
where the code asserted the opposite: *"A configure failure IS fatal: it means
the project itself is malformed, not that one component is unprobeable."*

`run_probes` now drops the components CMake NAMED and retries with the rest,
using CMake's own attribution (`CMake Error at <dir>/CMakeLists.txt:N`, where
`<dir>` is a component's `package_dir`) rather than a guess. An error naming
nothing keeps the old fatal behaviour, because a project really can be
malformed and picking a victim would be worse.

**In this workspace it changes no outcome** — the retry loop drains 16 → 12 →
11 → 5 → 4 → 1 and every component fails for the same missing package. That is
a property of `features`, not of the mechanism, and it is why this is filed
rather than closed.

### Cost, stated

| | before | after |
| --- | --- | --- |
| cold (markers cleared) | ~1 configure | 5.5 s, 6+ configures |
| warm (negative cache, issue 0641) | — | 0.10 s |

The cold path is ~5x more expensive on a workspace where nothing can configure,
paid once per source change because issue 0641's negative marker absorbs the
repeat. That is a real trade and it is made on the contract, not on speed: a
workspace with one broken component and sixteen good ones now gets sixteen
sidecars instead of none.

## What would actually fix it

Not decided, and both options are design steps:

* **build the workspace's interface packages inside the probe project.**
  `add_subdirectory` alone does not satisfy `find_package`, which wants a config
  file — so this means generating one, or CMake 3.24's
  `FetchContent … FIND_PACKAGE_ARGS`.
* **accept these as unprobeable and make it loud.** They currently degrade to
  the SystemModel bound, which is a real answer — just a coarser one. The
  argument for this option is that the probe exists to get an EXACT executor
  size, and a workspace whose messages are not built yet cannot give one.

Whichever is chosen, the count belongs somewhere visible: sixteen components on
the lower bound is not obviously wrong, and is currently invisible unless
someone runs `nros sync --verbose` and reads.
