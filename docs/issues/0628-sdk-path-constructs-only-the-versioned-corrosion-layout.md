---
id: 628
title: "`nros sdk-path corrosion` constructs only the VERSIONED layout, so a flat-layout install is ignored and every configure silently fetches Corrosion from GitHub"
status: open
type: bug
area: build/provisioning
related: [issue-0493, issue-0500, issue-0622, phase-365, phase-354]
---

## Symptom

A configure that has a provisioned Corrosion nevertheless reports it missing and
goes to the network:

```
-- nano-ros: Corrosion not provisioned — fetching v0.6.1 from git.
   Install it offline-safe with:  nros setup --tool corrosion
-- nano-ros: Corrosion v0.6.1 via FetchContent [hashed per-workspace cargo dirs]
   — examples/workspaces/mixed/build-workspace-fixtures/_deps/corrosion-src
```

The advice in that message is the thing that was already done.

## Measurement

```
$ nros sdk-path corrosion
/home/aeon/.nros/sdk/corrosion/0.6.1-nros1        # constructed
$ [ -d /home/aeon/.nros/sdk/corrosion/0.6.1-nros1 ] ; echo $?
1                                                  # does not exist

$ cat ~/.nros/sdk/corrosion/.installed-version
v0.6.1
$ ls ~/.nros/sdk/corrosion/lib/cmake/Corrosion/
CorrosionConfig.cmake  CorrosionConfigVersion.cmake
```

The pinned version IS installed and IS resolvable — under the FLAT layout.
`_nros_corrosion_store_dir()` constructs only the VERSIONED path, its
`IS_DIRECTORY` guard fails, `find_package` is never called, and the macro falls
through to `FetchContent`.

## Why this is a regression rather than a gap

`cmake/NanoRosCorrosion.cmake`'s own header says both layouts are supported, and
says exactly what happens when one is missed:

> Two install LAYOUTS exist and both are supported, because the two provisioning
> paths disagree:
>
>     just workspace install-corrosion   ->  $NROS_HOME/sdk/corrosion/          (flat)
>     nros setup --tool corrosion        ->  $NROS_HOME/sdk/corrosion/<version>/
>
> The pre-0493 root-CMakeLists block globbed `corrosion/*` only, which sees the
> VERSIONED layout and, under the FLAT one, yields `lib/` and `share/` — two
> prefixes `find_package` cannot resolve from. **That is why the SDK install was
> missed on a host that had it: not a provisioning step anyone forgot, an
> unsupported layout.**

That is this bug, one layout the other way, reintroduced by the phase-365 switch
from a searched prefix list (`_nros_corrosion_prefixes`, newest-version-first)
to a single constructed path. The header's warning survived the change; the
behaviour it describes did not.

## Impact

* **Configure requires the network.** An offline or air-gapped host now fails to
  configure where it previously used its provisioned copy. The build tree in
  which this was found fetched `corrosion-src` into `_deps/` on every
  from-scratch configure.
* **The remedy printed is a no-op.** `nros setup --tool corrosion` writes the
  versioned layout, so following the message would in fact fix it — but only by
  installing a second copy beside the working one, and the message gives no way
  to tell that the first is being ignored rather than absent.
* **It is silent.** Corrosion still resolves at v0.6.1 with the correct hashed
  per-workspace topology, so nothing downstream breaks and nothing says the
  provisioned copy lost. This is issue 0500's lesson in its exact words — a
  provisioning path that "prints success either way".

## Fix shape

The phase-365 thesis (an SDK path is CONSTRUCTED, not searched) is not in
question — searching is what let a stale prefix shadow a pin in 0500. The defect
is that ONE of the two shapes a provisioning run can leave behind is
constructible today. Options, in rough order of preference:

1. **Construct both and take the first that resolves** — `<store>/<version>/`
   then `<store>/`. Still construction, two candidates, no glob.
2. **Normalise at install time** so only one layout is ever written, and migrate
   or reject the other explicitly. Cleanest end state; needs a migration for
   existing hosts.
3. Have `nros sdk-path` itself answer with the layout that is actually present,
   so cmake keeps asking one question and gets a usable answer.

Whichever is taken, the `IS_DIRECTORY` guard should not fail SILENTLY into
`FetchContent`: a store that exists but does not match the constructed shape is
worth one `message(STATUS)` naming both paths, because that is the difference
between "not installed" and "installed where I did not look".

## Provenance

Found 2026-08-16 while verifying phase-354 W1's acceptance — the mixed native
workspace links, and the configure line it prints is how this surfaced. CLAUDE.md
requires that line to be READ rather than inferred, and reading it is what showed
the origin was `FetchContent` and not `SDK store`.
