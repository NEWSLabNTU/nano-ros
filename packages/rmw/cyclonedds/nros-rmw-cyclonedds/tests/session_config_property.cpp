// RFC-0109 D6 (phase-514 W3b) — a `[[domain]]`'s Cyclone XML reaches the
// session that joins it, as the one `config` session property.
//
// A bridge image opens a second domain the image's baked `rmw/cyclonedds.xml`
// does not describe, so that domain's own file travels with its session spec.
// What this pins:
//
//   1. a valid document opens the session;
//   2. the document is HANDED TO CYCLONE, not merely accepted — a malformed one
//      fails the open. Without this, a backend that took the property and
//      dropped it would pass (1);
//   3. every other key is still refused (phase-206 W3: a configuration line
//      that is silently dropped is indistinguishable from one that applied),
//      and two documents for one session are refused rather than one winning.

#include <cstdio>
#include <cstring>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
int g_bad = 0;

void expect(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        g_bad = 1;
    }
}

rmw_ret_t open_with(uint32_t domain, const rmw_session_property_t* props, size_t n,
                    rmw_session_t* s) {
    s->node_name = "session_config_property";
    s->namespace_ = "/";
    rmw_session_options_t opts{};
    opts.properties = props;
    opts.property_count = n;
    return g_vt->create_session(nullptr, 0, domain, s->node_name, &opts, s);
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }

    const char* good = "<CycloneDDS><Domain id=\"any\"><General>"
                       "<AllowMulticast>false</AllowMulticast>"
                       "</General></Domain></CycloneDDS>";
    rmw_session_property_t ok_prop{"config", good};
    rmw_session_t s{};
    expect(open_with(nros_test_domain(89), &ok_prop, 1, &s) == NROS_RMW_RET_OK,
           "a valid `config` document opens the session");
    (void)g_vt->destroy_session(&s);

    const char* broken = "<CycloneDDS><Domain id=\"any\"><General>"
                         "<AllowMulticast>maybe</AllowMulticast>"
                         "</General></Domain></CycloneDDS>";
    rmw_session_property_t bad_prop{"config", broken};
    rmw_session_t b{};
    rmw_ret_t r = open_with(nros_test_domain(90), &bad_prop, 1, &b);
    expect(r != NROS_RMW_RET_OK,
           "a malformed `config` document must fail the open: the document reaches "
           "Cyclone, it is not merely accepted");
    if (r == NROS_RMW_RET_OK) {
        (void)g_vt->destroy_session(&b);
    }

    rmw_session_property_t other{"listen", "tcp/0.0.0.0:7447"};
    rmw_session_t o{};
    expect(open_with(nros_test_domain(90), &other, 1, &o) == NROS_RMW_RET_UNSUPPORTED,
           "a key other than `config` is refused, never dropped");

    rmw_session_property_t two[2] = {{"config", good}, {"config", good}};
    rmw_session_t t{};
    expect(open_with(nros_test_domain(90), two, 2, &t) == NROS_RMW_RET_INVALID_ARGUMENT,
           "two documents for one session are refused, not resolved by order");

    if (g_bad == 0) {
        std::printf("session_config_property: OK (the document reaches Cyclone; "
                    "other keys and a second document refused)\n");
    }
    return g_bad;
}
