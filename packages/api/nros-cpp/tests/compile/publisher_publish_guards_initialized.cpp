// phase-417 stage 3 — `rclcpp::Publisher<M>::publish` must refuse an
// UNINITIALISED publisher.
//
// RUNTIME probe, not a syntax one, and the shape is forced by what the defect
// was. `publish` was the only entry point on the class without an
// `initialized_` guard: it called `M::ffi_publish(storage_, &msg)`
// unconditionally, and the runtime's only check is `storage.is_null()` while
// `storage_` is an in-object array that is never null. So the bytes of a
// default-constructed publisher were reinterpreted as an `RmwPublisher` —
// silently wrong, then UB, with no diagnostic at any warning level. Nothing a
// compiler can see; only a call can answer it.
//
// Two properties, and the second is the one a return-code check alone would
// miss:
//
//   1. the call returns `ErrorCode::NotInitialized`;
//   2. `M::ffi_publish` is NOT REACHED. A guard that returned the right code
//      after already handing zeroed storage to the runtime would satisfy (1)
//      and still be the bug, so the fake message records whether it was
//      entered.
//
// `M` is a local stand-in rather than a generated message on purpose: `publish`
// names nothing on `M` but `ffi_publish`, and a probe that needed codegen could
// not run in this lane at all (no compilation inside tests — the fixture would
// have to be built, and this defect lives in a header).
//
// Links no nano-ros archive: the four `nros_cpp_publisher_*` entry points the
// instantiated template references are DEFINED BELOW as recording stubs, so
// there is no unresolved FUNCTION symbol. That matters beyond convenience — a
// stub is what lets property (2) be asserted for the sibling verbs as well as
// for `publish`, and `--unresolved-symbols=ignore-all` (which the recipe passes
// for the issue-0360 variant anchors) cannot produce a RUNNABLE binary when a
// function is missing: the loader rejects it with `unexpected PLT reloc type`.

#include <cstdio>
#include <cstdlib>

#include <nros/nros.hpp>
#include <nros/publisher.hpp>

namespace {

bool g_ffi_publish_entered = false;
bool g_runtime_entered = false;

/// The smallest thing `rclcpp::Publisher<M>::publish` will accept.
struct FakeMsg {
    int value;

    static nros_cpp_ret_t ffi_publish(void* storage, const FakeMsg* msg) {
        (void)storage;
        (void)msg;
        g_ffi_publish_entered = true;
        return 0; // "published fine" — which is exactly the lie under test
    }
};

int failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ::std::fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

} // namespace

// The runtime seam, stubbed. Reaching ANY of these with zeroed storage is the
// defect, so every one of them records rather than pretending to work.
extern "C" {

nros_cpp_ret_t nros_cpp_publish_raw(void* storage, const uint8_t* data, size_t len) {
    (void)storage;
    (void)data;
    (void)len;
    g_runtime_entered = true;
    return 0;
}

nros_cpp_ret_t nros_cpp_publisher_loan(void* storage, size_t requested_len, uint8_t** out_buf,
                                       size_t* out_cap, void** out_token) {
    (void)storage;
    (void)requested_len;
    (void)out_buf;
    (void)out_cap;
    (void)out_token;
    g_runtime_entered = true;
    return 0;
}

nros_cpp_ret_t nros_cpp_publisher_discard(void* storage, void* token) {
    (void)storage;
    (void)token;
    g_runtime_entered = true;
    return 0;
}

nros_cpp_ret_t nros_cpp_publisher_destroy(void* storage) {
    (void)storage;
    g_runtime_entered = true;
    return 0;
}

nros_cpp_ret_t nros_cpp_publisher_assert_liveliness(void* storage) {
    (void)storage;
    g_runtime_entered = true;
    return 0;
}

} // extern "C"

int main() {
    // Never initialised. This is what `Node::create_publisher` leaves behind
    // when it fails, and what a ported node holds if it does not read the
    // `Result`.
    rclcpp::Publisher<FakeMsg> pub;
    check(!pub.is_valid(), "a default-constructed publisher reports is_valid() == false");

    FakeMsg msg;
    msg.value = 7;
    nros::Result r = pub.publish(msg);

    check(!r.ok(), "publish on an uninitialised publisher must not report success");
    check(r.code() == nros::ErrorCode::NotInitialized,
          "publish on an uninitialised publisher must return ErrorCode::NotInitialized");
    check(!g_ffi_publish_entered,
          "publish must not reach M::ffi_publish on an uninitialised publisher — "
          "the runtime would reinterpret zeroed storage as an RmwPublisher");

    // The sibling entry points, so a fix that guarded only the reported site
    // cannot pass: every one of these already had the guard, and this is what
    // keeps them having it.
    check(pub.publish_raw(reinterpret_cast<const uint8_t*>("x"), 1).code() ==
              nros::ErrorCode::NotInitialized,
          "publish_raw keeps its guard");
    check(pub.assert_liveliness().code() == nros::ErrorCode::NotInitialized,
          "assert_liveliness keeps its guard");
    check(!pub.loan(8).ok(), "loan keeps its guard");
    check(!g_runtime_entered,
          "no entry point on an uninitialised publisher may reach the runtime seam");

    if (failures != 0) {
        ::std::fprintf(stderr, "publisher_publish_guards_initialized: %d failure(s)\n", failures);
        return 1;
    }
    ::std::printf("publisher_publish_guards_initialized: OK\n");
    return 0;
}
