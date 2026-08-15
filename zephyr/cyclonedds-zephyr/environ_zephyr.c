/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Zephyr replacement for
 *   third-party/dds/cyclonedds/src/ddsrt/src/environ/posix/environ.c
 *
 * Issue 0590. The POSIX TU calls `setenv`/`unsetenv`, which Zephyr's minimal
 * libc declares only behind its POSIX options (`CONFIG_POSIX_API` /
 * `_POSIX_C_SOURCE`). Without those the calls are implicitly declared, and the
 * gcc 14 the Zephyr SDK 0.16.8 ships makes that an ERROR rather than a warning,
 * so every cyclonedds cell failed to compile. Because the Zephyr fixture make
 * driver has no `-k`, the build stopped at the cyclone group and no later
 * Zephyr fixture was produced at all — which is how a cyclone-only compile
 * error turned tier 1 red on any host with a provisioned Zephyr workspace.
 *
 * Why a backend rather than `CONFIG_POSIX_API=y`: turning the POSIX surface on
 * to obtain two functions drags in the option set issue 0566 is about, and
 * Zephyr's POSIX objects come from fixed static pools (issues 0371 / 0496) —
 * the very coupling the native `k_mutex`/`k_condvar` sync backend exists to
 * escape. Adding a second reason to depend on that surface would undo it.
 *
 * WHAT THIS PROVIDES, and its one deliberate limitation: a process-global
 * environment does not exist on Zephyr. Every image is a single application
 * built with its configuration COMPILED IN, so there is nothing for a runtime
 * `setenv` to influence — `nros` bakes the locator, domain and node name at
 * build time precisely because the guest has no populated environment
 * (`option_env!("NROS_LOCATOR")` and friends in the board crates).
 *
 * So `getenv` reports NOT_FOUND and the mutators report OK without storing.
 * OK, not an error: cyclone calls `ddsrt_setenv` on paths that must not fail
 * (config expansion, `CYCLONEDDS_URI` handling), and returning an error there
 * would convert "nothing to configure" into an init failure. The values are
 * unreadable by construction — the only reader is `ddsrt_getenv`, which is in
 * this file and reports NOT_FOUND — so nothing can observe a write that was
 * silently dropped and conclude it took effect.
 *
 * Argument validation matches the POSIX TU exactly (empty name, or a name
 * containing '='), so callers see the same BAD_PARAMETER contract.
 */

#include <assert.h>
#include <string.h>

#include "dds/ddsrt/environ.h"
#include "dds/ddsrt/retcode.h"

/* Same predicate as the POSIX backend: a name is valid when it is non-empty
 * and contains no '='. */
static int isenvvar(const char *name)
{
    return (*name == '\0' || strchr(name, '=') != NULL) == 0;
}

dds_return_t ddsrt_getenv(const char *name, const char **value)
{
    assert(name != NULL);
    assert(value != NULL);

    if (!isenvvar(name))
        return DDS_RETCODE_BAD_PARAMETER;

    /* No process environment on Zephyr — see the header comment. Callers treat
     * NOT_FOUND as "unset", which is the truth here. */
    return DDS_RETCODE_NOT_FOUND;
}

dds_return_t ddsrt_setenv(const char *name, const char *value)
{
    assert(name != NULL);
    assert(value != NULL);

    /* The POSIX TU routes an empty value to unsetenv; keep that ordering so the
     * BAD_PARAMETER cases coincide. */
    if (strlen(value) == 0)
        return ddsrt_unsetenv(name);
    if (!isenvvar(name))
        return DDS_RETCODE_BAD_PARAMETER;

    return DDS_RETCODE_OK;
}

dds_return_t ddsrt_unsetenv(const char *name)
{
    assert(name != NULL);

    if (!isenvvar(name))
        return DDS_RETCODE_BAD_PARAMETER;

    return DDS_RETCODE_OK;
}
