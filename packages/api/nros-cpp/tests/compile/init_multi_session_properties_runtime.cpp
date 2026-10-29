// Issue 2123 — a C++ multi-session image hands EACH session its own RMW
// properties (its domain's RMW file, lowered on the host, RFC-0109 D8).
//
// `NrosCppSessionSpec` carried no properties, so `nros_cpp_init_multi` opened
// every session with an empty list: a bridged domain's `config` could reach a
// Rust image (`nros-bridge.toml`) and never a C or C++ one. A compile check
// cannot tell "the field exists" from "the rows reach the backend", and it
// cannot tell "session 1 got session 1's rows" from "every session got the
// first spec's", which is the mistake a shared pool invites. So this RUNS it,
// against the shared stub backend, which records the properties each session
// was opened with (copies: the runtime lends the rows for the open only).
//
// Asserted:
//   1. two specs, two backends, two DIFFERENT tables -> each session saw its
//      own rows, in order, and a spec with none saw none;
//   2. a NULL table with a non-zero length is refused before anything opens;
//   3. more rows than the call can carry is refused (FULL), never truncated.

#include "rclcpp/detail/nros_cpp_ffi.h"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <vector>

// The strong hook `nros_cpp_init_multi` calls; there is no weak default that
// would register the stubs for us.
extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    (void)nros_stub_rmw_register_as("stub_b");
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

bool row_is(uint32_t session, size_t i, const char* key, const char* value) {
    const char* k = nullptr;
    const char* v = nullptr;
    return nros_stub_rmw_session_property(session, i, &k, &v) && std::strcmp(k, key) == 0 &&
           std::strcmp(v, value) == 0;
}

NrosCppSessionSpec spec(const char* rmw, const nros_cpp_session_property_t* rows, size_t len) {
    return NrosCppSessionSpec{rmw, "", 0, "probe", nullptr, rows, len};
}

const nros_cpp_session_property_t kEcu[] = {
    {"mode", "client"},
    {"connect", "tcp/10.0.0.1:7447"},
};
const nros_cpp_session_property_t kCtl[] = {
    {"config", "<CycloneDDS><Domain id=\"any\"/></CycloneDDS>"},
};

} // namespace

int main() {
    std::vector<uint64_t> storage(nros_cpp_executor_storage_size() / 8 + 1, 0);

    // 1. Each session gets its own table; a spec with none gets none.
    {
        const uint32_t first = nros_stub_rmw_sessions_opened();
        NrosCppSessionSpec specs[3] = {
            spec(NROS_STUB_RMW_NAME, kEcu, 2),
            spec("stub_b", kCtl, 1),
            spec("stub_c", nullptr, 0),
        };
        (void)nros_stub_rmw_register_as("stub_c");
        nros_cpp_ret_t rc = nros_cpp_init_multi(specs, 3, storage.data());
        check(rc == NROS_CPP_RET_OK, "three specs open");
        if (rc == NROS_CPP_RET_OK) {
            check(nros_stub_rmw_sessions_opened() == first + 3, "three sessions opened");
            check(nros_stub_rmw_session_property_count(first) == 2, "session 0 saw two rows");
            check(row_is(first, 0, "mode", "client"), "session 0 row 0 is its own");
            check(row_is(first, 1, "connect", "tcp/10.0.0.1:7447"), "session 0 row 1 is its own");
            check(nros_stub_rmw_session_property_count(first + 1) == 1, "session 1 saw one row");
            check(row_is(first + 1, 0, "config", "<CycloneDDS><Domain id=\"any\"/></CycloneDDS>"),
                  "session 1 saw ITS table, not session 0's");
            check(nros_stub_rmw_session_property_count(first + 2) == 0,
                  "a spec with no table opens with none");
            check(nros_cpp_fini(storage.data()) == NROS_CPP_RET_OK, "fini");
        }
    }

    // 2. A NULL table with rows is refused before any session opens.
    {
        const uint32_t before = nros_stub_rmw_sessions_opened();
        NrosCppSessionSpec specs[1] = {spec(NROS_STUB_RMW_NAME, nullptr, 1)};
        check(nros_cpp_init_multi(specs, 1, storage.data()) == NROS_CPP_RET_INVALID_ARGUMENT,
              "NULL rows with a length are INVALID_ARGUMENT");
        check(nros_stub_rmw_sessions_opened() == before, "nothing opened for a refused table");
    }

    // 3. More rows than one session open carries: FULL, never a truncation.
    {
        const uint32_t before = nros_stub_rmw_sessions_opened();
        std::vector<nros_cpp_session_property_t> many(17, nros_cpp_session_property_t{"k", "v"});
        NrosCppSessionSpec specs[1] = {spec(NROS_STUB_RMW_NAME, many.data(), many.size())};
        check(nros_cpp_init_multi(specs, 1, storage.data()) == NROS_CPP_RET_FULL,
              "17 rows for one session are FULL");
        check(nros_stub_rmw_sessions_opened() == before, "nothing opened for an oversized table");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "init_multi_session_properties_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("init_multi_session_properties_runtime: OK\n");
    return 0;
}
