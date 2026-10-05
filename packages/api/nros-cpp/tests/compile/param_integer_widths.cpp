// POSITIVE compile probe — issue 1678: every scalar type `node_param_type`
// classifies is a type the parameter calls TAKE, on every data model.
//
// `node_param_type<T>` (the contract check's table) called `long long`,
// `unsigned`, `short`, `float` & co. INTEGER / DOUBLE, while the store's
// overload set took only `bool`, `int`, `int64_t`, `double` and `const char*`.
// So the check accepted types the calls could not bind, and WHICH ones depended
// on the data model: `int64_t` is `long` on LP64 and `long long` on ILP32, so
// `declare_parameter<long long>` compiled on a Cortex-M and failed on the host
// (ambiguous declare, no viable get), and `declare_parameter<long>` did the
// reverse. A header whose callers write rclcpp-style code once must not have a
// portable direction.
//
// Compiled in THREE arms by `just check cpp`: hosted LP64, `-nostdinc++`
// against the ThreadX shim (LP64), and the same freestanding arm for an ILP32
// target (`clang++ --target=armv7a-none-eabi`), where `long` is 32-bit and
// `int64_t` is `long long` — the arm in which the overload set was right
// for `long long` and wrong for `long`. `NROS_CPP_STD` is NOT defined: the
// `std::vector` forms are pinned by `param_hosted_overloads.cpp`.
//
// The `static_assert`s are the RANGE rule, evaluated by the compiler: a stored
// integer reads back into a narrower `T` only if it fits, and an unsigned
// declaration above INT64_MAX is refused rather than wrapped negative.

#include <nros/nros.hpp>

namespace nros_cpp_param_integer_widths_test {

namespace d = ::nros::detail;

// --- the range rule ---------------------------------------------------------
static_assert(d::node_param_fits<short>(32767), "short max fits");
static_assert(!d::node_param_fits<short>(32768), "short max+1 refused");
static_assert(!d::node_param_fits<unsigned int>(-1), "negative into unsigned refused");
static_assert(d::node_param_fits<unsigned int>(4294967295LL), "uint32 max fits");
static_assert(!d::node_param_fits<unsigned int>(4294967296LL), "uint32 max+1 refused");
// The one the round trip alone cannot see: same width, different sign.
static_assert(!d::node_param_fits<unsigned long long>(-1), "negative into uint64 refused");
static_assert(d::node_param_fits<long long>(-1), "long long takes negatives");
static_assert(d::node_param_widens<unsigned long long>(9223372036854775807ULL), "INT64_MAX widens");
static_assert(!d::node_param_widens<unsigned long long>(9223372036854775808ULL),
              "INT64_MAX+1 is refused, not wrapped negative");
static_assert(d::node_param_widens<long long>(-5), "signed always widens");

// --- the table and the calls agree ------------------------------------------
template <typename T> inline void scalar(rclcpp::Node& node, const char* name, T v) {
    T in_effect = node.declare_parameter<T>(name, v);
    T out = T();
    (void)node.get_parameter<T>(name, out);
    (void)node.get_parameter<T>(name);
    (void)node.get_parameter_or<T>(name, out, v);
    (void)node.set_parameter<T>(name, in_effect).ok();
}

template <typename T> inline void seq(rclcpp::Node& node, const char* name) {
    nros::Seq<T, 4> v;
    (void)v.push_back(T(1));
    (void)node.declare_parameter<nros::Seq<T, 4>>(name, v);
    nros::Seq<T, 4> out;
    (void)node.get_parameter<nros::Seq<T, 4>>(name, out);
    (void)node.set_parameter<nros::Seq<T, 4>>(name, v).ok();
}

inline void instantiate(rclcpp::Node& node) {
    scalar<bool>(node, "b", true);
    scalar<char>(node, "c", 'x');
    scalar<signed char>(node, "sc", 1);
    scalar<unsigned char>(node, "uc", 1);
    scalar<short>(node, "s", 1);
    scalar<unsigned short>(node, "us", 1);
    scalar<int>(node, "i", 1);
    scalar<unsigned int>(node, "ui", 1u);
    scalar<long>(node, "l", 1L);
    scalar<unsigned long>(node, "ul", 1UL);
    scalar<long long>(node, "ll", 1LL); // issue 1678's report: LP64 failed here
    scalar<unsigned long long>(node, "ull", 1ULL);
    scalar<int32_t>(node, "i32", 1);
    scalar<int64_t>(node, "i64", 1);
    scalar<uint8_t>(node, "u8", 1);
    scalar<float>(node, "f", 1.0f);
    scalar<double>(node, "d", 1.0);

    seq<bool>(node, "sb");
    seq<int>(node, "si");
    seq<long>(node, "sl");
    seq<long long>(node, "sll");
    seq<int64_t>(node, "si64");
    seq<float>(node, "sf");
    seq<double>(node, "sd");
}

#ifdef NROS_CPP_STD
// The hosted array form, compiled by the `-DNROS_CPP_STD` arm: a
// `std::vector<int>` / `<long long>` / `<float>` is INTEGER_ARRAY /
// DOUBLE_ARRAY in the table, so it must bind to the store too.
template <typename T> inline void vec(rclcpp::Node& node, const char* name) {
    std::vector<T> v(2, T(1));
    (void)node.declare_parameter<std::vector<T>>(name, v);
    std::vector<T> out;
    (void)node.get_parameter<std::vector<T>>(name, out);
    (void)node.set_parameter<std::vector<T>>(name, v).ok();
}

inline void instantiate_hosted(rclcpp::Node& node) {
    vec<int>(node, "vi");
    vec<long>(node, "vl");
    vec<long long>(node, "vll");
    vec<int64_t>(node, "vi64");
    vec<unsigned>(node, "vu");
    vec<float>(node, "vf");
    vec<double>(node, "vd");
}
#endif // NROS_CPP_STD

} // namespace nros_cpp_param_integer_widths_test
