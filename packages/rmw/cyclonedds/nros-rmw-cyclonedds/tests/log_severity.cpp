// Issue 0800 — `set_log_severity` had a slot, a runtime dispatcher and stub
// tests since phase-376 W5, and NO backend body, so every real image answered
// UNSUPPORTED. This asserts the Cyclone body: that the slot is filled, and that
// the ladder maps onto Cyclone's CATEGORY BITMASK the way the header claims.
//
// Reads the mask back with `dds_get_log_mask()` rather than trusting the call's
// return code — the return says the call was made, the mask says what it did.
// That distinction is the one issue 0803 was lost in for a day.
//
// No session, no router: `dds_set_log_mask` is process-global state in ddsrt.

#include <cstdio>

#include <dds/ddsrt/log.h>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;

int check(const char *what, rmw_log_severity_t sev, uint32_t want) {
    rmw_ret_t rc = g_vt->set_log_severity(sev);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: %s returned %d\n", what, (int)rc);
        return 1;
    }
    uint32_t got = dds_get_log_mask();
    if (got != want) {
        std::fprintf(stderr, "FAIL: %s left mask 0x%x, expected 0x%x\n", what, got, want);
        return 1;
    }
    return 0;
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char * /*name*/,
                                                  const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "FAIL: backend did not register\n");
        return 1;
    }
    if (g_vt->set_log_severity == nullptr) {
        std::fprintf(stderr, "FAIL: set_log_severity slot is NULL — the capability is declared "
                             "and unimplemented (issue 0800)\n");
        return 2;
    }

    int bad = 0;
    // Cumulative: each severity enables itself and everything more urgent.
    bad |= check("FATAL", RMW_LOG_SEVERITY_FATAL, DDS_LC_FATAL);
    bad |= check("ERROR", RMW_LOG_SEVERITY_ERROR, DDS_LC_FATAL | DDS_LC_ERROR);
    bad |= check("WARN", RMW_LOG_SEVERITY_WARN, DDS_LC_FATAL | DDS_LC_ERROR | DDS_LC_WARNING);
    bad |= check("INFO", RMW_LOG_SEVERITY_INFO,
                 DDS_LC_FATAL | DDS_LC_ERROR | DDS_LC_WARNING | DDS_LC_INFO);
    bad |= check("DEBUG", RMW_LOG_SEVERITY_DEBUG, DDS_LC_ALL);

    // UNSET states no severity, so it is refused rather than guessed — a
    // backend inventing one would pick a verbosity nobody asked for.
    uint32_t before = dds_get_log_mask();
    if (g_vt->set_log_severity(RMW_LOG_SEVERITY_UNSET) != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "FAIL: UNSET must be refused with INVALID_ARGUMENT\n");
        bad = 1;
    }
    if (dds_get_log_mask() != before) {
        std::fprintf(stderr, "FAIL: a refused severity must not change the mask\n");
        bad = 1;
    }

    if (bad == 0) {
        std::printf("log_severity: OK (5 level(s) mapped, UNSET refused)\n");
    }
    return bad;
}
