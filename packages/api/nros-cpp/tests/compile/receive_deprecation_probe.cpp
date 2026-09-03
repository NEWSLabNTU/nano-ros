/*
 * Phase 379 W6 decision 1 — the NEGATIVE half of `receive_verb_aliases.cpp`.
 *
 * That file proves the old `try_recv*` spellings still COMPILE. This one proves
 * they still WARN. A deprecation nobody is told about is just an alias, and the
 * maintainer's policy for this campaign is `[[deprecated]]` now and removal
 * later — so "the attribute reaches callers" is the thing worth pinning, not an
 * implementation detail.
 *
 * `just check cpp` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warnings into errors. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 *
 * Same shape as the C half, `packages/api/nros-c/tests/compile/
 * receive_deprecation_probe.c`.
 */

#include <nros/nros.hpp>

namespace nros_cpp_receive_deprecation_probe {

struct Int32 {
    int32_t data{0};
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

inline ::nros::Result probe(::nros::Subscription<Int32>& sub) {
    Int32 msg{};
    return sub.try_recv(msg);
}

} // namespace nros_cpp_receive_deprecation_probe
