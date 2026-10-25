// nros-cpp: upstream's rclcpp::ParameterValue and rclcpp::Parameter
// Freestanding C++ — the scalar forms; the std::string / std::vector forms are hosted

/**
 * @file parameter_value.hpp
 * @ingroup grp_parameter
 * @brief `rclcpp::ParameterValue` and `rclcpp::Parameter` (issue 2056).
 *
 * Ported code builds a parameter as a VALUE of any parameter type and reads
 * one back the same way: `declare_parameter(name, rclcpp::ParameterValue(
 * std::vector<double>{0.5, 0.0, 2.5}))`, and an on-set callback that walks a
 * `std::vector<rclcpp::Parameter>` asking `get_type()` and `as_double_array()`.
 * Measured over Autoware 0.45.1, 183 source files name
 * `std::vector<rclcpp::Parameter>`.
 *
 * These are VALUE objects, not a store. The executor's `nros_params` table
 * stays the one store (phase-426 W4): `Node::declare_parameter(name,
 * ParameterValue)` dispatches on the tag to the typed `declare_parameter<T>`,
 * and the on-set callback's `Parameter` is built from the store's proposed
 * write for the duration of the call.
 *
 * ## Layout
 *
 * Both types exist in every arm with one layout (`check-cpp-capability-layout`):
 * a type tag, the three scalar slots, and ONE pointer to a heap block that only
 * a hosted build ever allocates. A freestanding build gets the scalar
 * constructors and accessors and a `heap_` that is always null; the
 * `std::string` / `std::vector` forms are hosted METHODS, never members.
 *
 * ## What it cannot hold
 *
 * The store's array FFI carries bool, integer and double arrays. A
 * `ParameterValue` can still HOLD a byte or string array, so ported code that
 * builds one compiles, but declaring or setting one through a node answers
 * `NOT_SET` / `Unsupported`, never a silent empty array.
 *
 * ## Wrong-type reads
 *
 * Upstream throws `ParameterTypeException` from `as_double()` on an integer
 * value. There are no exceptions here (RFC-0018), so a wrong-type read answers
 * the type's zero value: `0`, `0.0`, `false`, an empty string or vector. That
 * is the same answer `get_parameter<T>(name)` gives an undeclared name. Code
 * that asks `get_type()` first, which is how upstream code is written, sees no
 * difference.
 */

#ifndef NROS_RCLCPP_PARAMETER_VALUE_HPP
#define NROS_RCLCPP_PARAMETER_VALUE_HPP

#include <cstddef>
#include <cstdint>
#include <stddef.h>

#include "rclcpp/node_parameters.hpp" // rclcpp::ParameterType, ParameterWrite
#include "rclcpp/detail/traits.hpp"

#ifdef NROS_CPP_STD // hosted-family: parameter-interop
#include <string>
#include <utility>
#include <vector>
// Upstream's `rclcpp/parameter_value.hpp`, `parameter.hpp` and
// `node_interfaces/node_parameters_interface.hpp` include these, so code that
// includes `rclcpp/rclcpp.hpp` names `rcl_interfaces::msg::ParameterDescriptor`
// or `ParameterType` without including them (Nav2's `node_utils.hpp` and
// `velocity_smoother.cpp` do). They are generated in the USER's build when the
// package depends on `rcl_interfaces`, so they are included only when that
// build put them on the include path.
#if defined(__has_include)
#if __has_include("rcl_interfaces/msg/parameter_type.hpp")
#include "rcl_interfaces/msg/parameter_type.hpp"
#endif
#if __has_include("rcl_interfaces/msg/parameter_value.hpp")
#include "rcl_interfaces/msg/parameter_value.hpp"
#endif
#if __has_include("rcl_interfaces/msg/parameter.hpp")
#include "rcl_interfaces/msg/parameter.hpp"
#endif
#if __has_include("rcl_interfaces/msg/parameter_descriptor.hpp")
#include "rcl_interfaces/msg/parameter_descriptor.hpp"
#endif
#if __has_include("rcl_interfaces/msg/set_parameters_result.hpp")
#include "rcl_interfaces/msg/set_parameters_result.hpp"
#endif
#if __has_include("rcl_interfaces/msg/list_parameters_result.hpp")
#include "rcl_interfaces/msg/list_parameters_result.hpp"
#endif
#endif
#endif

namespace rclcpp {

namespace detail {
/// The hosted payload of a `ParameterValue`: whichever of these the tag names.
/// Declared everywhere, DEFINED only on a hosted build, so a freestanding
/// `ParameterValue` carries a pointer to an incomplete type that is never set.
struct ParameterValueHeap;
/// The hosted payload of a `Parameter`: its name.
struct ParameterNameHeap;

inline ParameterValueHeap* param_value_heap_clone(const ParameterValueHeap* h);
inline void param_value_heap_free(ParameterValueHeap* h);
inline ParameterNameHeap* param_name_heap_clone(const ParameterNameHeap* h);
inline void param_name_heap_free(ParameterNameHeap* h);

template <typename T, int Code> struct param_value_scalar_get;
template <typename T> struct param_value_get;
} // namespace detail

/// `rclcpp::ParameterValue` — a value of any parameter type.
class ParameterValue {
  public:
    /// A value of type `PARAMETER_NOT_SET`.
    ParameterValue() : type_(PARAMETER_NOT_SET), bool_(false), integer_(0), double_(0.0), heap_() {}
    explicit ParameterValue(bool v) : ParameterValue() {
        type_ = PARAMETER_BOOL;
        bool_ = v;
    }
    explicit ParameterValue(int v) : ParameterValue(static_cast<long long>(v)) {}
    explicit ParameterValue(long v) : ParameterValue(static_cast<long long>(v)) {}
    explicit ParameterValue(long long v) : ParameterValue() {
        type_ = PARAMETER_INTEGER;
        integer_ = static_cast<int64_t>(v);
    }
    explicit ParameterValue(float v) : ParameterValue(static_cast<double>(v)) {}
    explicit ParameterValue(double v) : ParameterValue() {
        type_ = PARAMETER_DOUBLE;
        double_ = v;
    }

    ParameterValue(const ParameterValue& o)
        : type_(o.type_), bool_(o.bool_), integer_(o.integer_), double_(o.double_),
          heap_(detail::param_value_heap_clone(o.heap_)) {}
    ParameterValue(ParameterValue&& o) noexcept
        : type_(o.type_), bool_(o.bool_), integer_(o.integer_), double_(o.double_), heap_(o.heap_) {
        o.heap_ = nullptr;
        o.type_ = PARAMETER_NOT_SET;
    }
    ParameterValue& operator=(const ParameterValue& o) {
        if (this != &o) {
            ParameterValue copy(o);
            swap(copy);
        }
        return *this;
    }
    ParameterValue& operator=(ParameterValue&& o) noexcept {
        if (this != &o) {
            ParameterValue moved(static_cast<ParameterValue&&>(o));
            swap(moved);
        }
        return *this;
    }
    ~ParameterValue() { detail::param_value_heap_free(heap_); }

    /// The value's type.
    ParameterType get_type() const { return static_cast<ParameterType>(type_); }

    /// The bool value; `false` for any other type.
    bool as_bool() const { return type_ == PARAMETER_BOOL ? bool_ : false; }
    /// The integer value; `0` for any other type.
    int64_t as_int() const { return type_ == PARAMETER_INTEGER ? integer_ : 0; }
    /// The double value; `0.0` for any other type.
    double as_double() const { return type_ == PARAMETER_DOUBLE ? double_ : 0.0; }

    /// `get<T>()` — the value as `T`: `bool`, any integer, any floating-point
    /// type, and on a hosted build `std::string` and the `std::vector` forms.
    template <typename T> T get() const { return detail::param_value_get<T>::get(*this); }

#ifdef NROS_CPP_STD // hosted-family: parameter-interop
    explicit ParameterValue(const char* v) : ParameterValue(::std::string(v)) {}
    explicit ParameterValue(const ::std::string& v);
    explicit ParameterValue(const ::std::vector<uint8_t>& v);
    explicit ParameterValue(const ::std::vector<bool>& v);
    explicit ParameterValue(const ::std::vector<int>& v);
    explicit ParameterValue(const ::std::vector<int64_t>& v);
    explicit ParameterValue(const ::std::vector<float>& v);
    explicit ParameterValue(const ::std::vector<double>& v);
    explicit ParameterValue(const ::std::vector<::std::string>& v);

    const ::std::string& as_string() const;
    const ::std::vector<uint8_t>& as_byte_array() const;
    const ::std::vector<bool>& as_bool_array() const;
    const ::std::vector<int64_t>& as_integer_array() const;
    const ::std::vector<double>& as_double_array() const;
    const ::std::vector<::std::string>& as_string_array() const;

    bool operator==(const ParameterValue& o) const;
    bool operator!=(const ParameterValue& o) const { return !(*this == o); }
#else
    /// Without this a string literal would pick the `bool` constructor
    /// through the pointer conversion. A freestanding build has no string
    /// value to build, so it refuses at compile time instead.
    explicit ParameterValue(const char* v) = delete;
#endif

  private:
    friend class Parameter;

    void swap(ParameterValue& o) noexcept {
        int32_t t = type_;
        type_ = o.type_;
        o.type_ = t;
        bool b = bool_;
        bool_ = o.bool_;
        o.bool_ = b;
        int64_t i = integer_;
        integer_ = o.integer_;
        o.integer_ = i;
        double d = double_;
        double_ = o.double_;
        o.double_ = d;
        detail::ParameterValueHeap* h = heap_;
        heap_ = o.heap_;
        o.heap_ = h;
    }

    int32_t type_;
    bool bool_;
    int64_t integer_;
    double double_;
    /// Owned; null unless a hosted constructor stored a string or an array.
    detail::ParameterValueHeap* heap_;
};

/// `rclcpp::Parameter` — a name plus a `ParameterValue`.
class Parameter {
  public:
    /// An unnamed `PARAMETER_NOT_SET` parameter.
    Parameter() : value_(), name_() {}
    Parameter(const Parameter& o)
        : value_(o.value_), name_(detail::param_name_heap_clone(o.name_)) {}
    Parameter(Parameter&& o) noexcept
        : value_(static_cast<ParameterValue&&>(o.value_)), name_(o.name_) {
        o.name_ = nullptr;
    }
    Parameter& operator=(const Parameter& o) {
        if (this != &o) {
            value_ = o.value_;
            detail::ParameterNameHeap* n = detail::param_name_heap_clone(o.name_);
            detail::param_name_heap_free(name_);
            name_ = n;
        }
        return *this;
    }
    Parameter& operator=(Parameter&& o) noexcept {
        if (this != &o) {
            value_ = static_cast<ParameterValue&&>(o.value_);
            detail::param_name_heap_free(name_);
            name_ = o.name_;
            o.name_ = nullptr;
        }
        return *this;
    }
    ~Parameter() { detail::param_name_heap_free(name_); }

    ParameterType get_type() const { return value_.get_type(); }
    const ParameterValue& get_parameter_value() const { return value_; }
    bool as_bool() const { return value_.as_bool(); }
    int64_t as_int() const { return value_.as_int(); }
    double as_double() const { return value_.as_double(); }
    /// `get_value<T>()` — `ParameterValue::get<T>()` on this parameter's value.
    template <typename T> T get_value() const { return value_.template get<T>(); }

#ifdef NROS_CPP_STD // hosted-family: parameter-interop
    explicit Parameter(const ::std::string& name);
    Parameter(const ::std::string& name, const ParameterValue& value);
    /// `Parameter(name, value)` for any type `ParameterValue` is constructible from.
    template <typename T>
    Parameter(const ::std::string& name, const T& value) : Parameter(name, ParameterValue(value)) {}

    /// The proposed write an on-set callback sees, as a `Parameter`.
    static Parameter from_write(const ParameterWrite& w);

    const ::std::string& get_name() const;
    const char* get_type_name() const;
    const ::std::string& as_string() const { return value_.as_string(); }
    const ::std::vector<uint8_t>& as_byte_array() const { return value_.as_byte_array(); }
    const ::std::vector<bool>& as_bool_array() const { return value_.as_bool_array(); }
    const ::std::vector<int64_t>& as_integer_array() const { return value_.as_integer_array(); }
    const ::std::vector<double>& as_double_array() const { return value_.as_double_array(); }
    const ::std::vector<::std::string>& as_string_array() const { return value_.as_string_array(); }

    bool operator==(const Parameter& o) const;
    bool operator!=(const Parameter& o) const { return !(*this == o); }
#endif

  private:
    ParameterValue value_;
    /// Owned; null unless a hosted constructor named the parameter.
    detail::ParameterNameHeap* name_;
};

namespace detail {

template <typename T> struct param_value_scalar_get<T, ::rclcpp::param_type::BOOL> {
    static T get(const ParameterValue& v) { return v.as_bool(); }
};
template <typename T> struct param_value_scalar_get<T, ::rclcpp::param_type::INTEGER> {
    static T get(const ParameterValue& v) { return static_cast<T>(v.as_int()); }
};
template <typename T> struct param_value_scalar_get<T, ::rclcpp::param_type::DOUBLE> {
    static T get(const ParameterValue& v) { return static_cast<T>(v.as_double()); }
};
/// Scalars by the parameter-type table; anything that table calls a string
/// and that has no specialization below is a compile error here.
template <typename T>
struct param_value_get : param_value_scalar_get<T, node_param_type<T>::value> {};
template <> struct param_value_get<ParameterValue> {
    static ParameterValue get(const ParameterValue& v) { return v; }
};

#ifdef NROS_CPP_STD // hosted-family: parameter-interop

struct ParameterValueHeap {
    ::std::string string_value;
    ::std::vector<uint8_t> byte_array;
    ::std::vector<bool> bool_array;
    ::std::vector<int64_t> integer_array;
    ::std::vector<double> double_array;
    ::std::vector<::std::string> string_array;
};
struct ParameterNameHeap {
    ::std::string name;
};

inline ParameterValueHeap* param_value_heap_clone(const ParameterValueHeap* h) {
    return h == nullptr ? nullptr : new ParameterValueHeap(*h);
}
inline void param_value_heap_free(ParameterValueHeap* h) {
    delete h;
}
inline ParameterNameHeap* param_name_heap_clone(const ParameterNameHeap* h) {
    return h == nullptr ? nullptr : new ParameterNameHeap(*h);
}
inline void param_name_heap_free(ParameterNameHeap* h) {
    delete h;
}

template <> struct param_value_get<::std::string> {
    static ::std::string get(const ParameterValue& v) { return v.as_string(); }
};
template <> struct param_value_get<::std::vector<uint8_t>> {
    static ::std::vector<uint8_t> get(const ParameterValue& v) { return v.as_byte_array(); }
};
template <> struct param_value_get<::std::vector<bool>> {
    static ::std::vector<bool> get(const ParameterValue& v) { return v.as_bool_array(); }
};
template <> struct param_value_get<::std::vector<::std::string>> {
    static ::std::vector<::std::string> get(const ParameterValue& v) { return v.as_string_array(); }
};
/// Any other numeric vector converts element by element from the stored array
/// its element type maps to (`std::vector<int>` from the integer array).
template <typename E, typename A> struct param_value_get<::std::vector<E, A>> {
    static ::std::vector<E, A> get(const ParameterValue& v) {
        ::std::vector<E, A> out;
        if (node_param_type<E>::value == ::rclcpp::param_type::DOUBLE) {
            for (double d : v.as_double_array()) {
                out.push_back(static_cast<E>(d));
            }
        } else {
            for (int64_t i : v.as_integer_array()) {
                out.push_back(static_cast<E>(i));
            }
        }
        return out;
    }
};

#else

inline ParameterValueHeap* param_value_heap_clone(const ParameterValueHeap*) {
    return nullptr;
}
inline void param_value_heap_free(ParameterValueHeap*) {}
inline ParameterNameHeap* param_name_heap_clone(const ParameterNameHeap*) {
    return nullptr;
}
inline void param_name_heap_free(ParameterNameHeap*) {}

#endif

} // namespace detail

#ifdef NROS_CPP_STD // hosted-family: parameter-interop

namespace detail {
template <typename T>
inline ParameterValueHeap* param_value_heap_with(::std::vector<T> ParameterValueHeap::*field,
                                                 ::std::vector<T> v) {
    ParameterValueHeap* h = new ParameterValueHeap();
    h->*field = static_cast<::std::vector<T>&&>(v);
    return h;
}
template <typename T> inline const T& param_value_empty() {
    static const T empty{};
    return empty;
}
} // namespace detail

inline ParameterValue::ParameterValue(const ::std::string& v) : ParameterValue() {
    type_ = PARAMETER_STRING;
    heap_ = new detail::ParameterValueHeap();
    heap_->string_value = v;
}
inline ParameterValue::ParameterValue(const ::std::vector<uint8_t>& v) : ParameterValue() {
    type_ = PARAMETER_BYTE_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::byte_array, v);
}
inline ParameterValue::ParameterValue(const ::std::vector<bool>& v) : ParameterValue() {
    type_ = PARAMETER_BOOL_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::bool_array, v);
}
inline ParameterValue::ParameterValue(const ::std::vector<int>& v) : ParameterValue() {
    type_ = PARAMETER_INTEGER_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::integer_array,
                                          ::std::vector<int64_t>(v.begin(), v.end()));
}
inline ParameterValue::ParameterValue(const ::std::vector<int64_t>& v) : ParameterValue() {
    type_ = PARAMETER_INTEGER_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::integer_array, v);
}
inline ParameterValue::ParameterValue(const ::std::vector<float>& v) : ParameterValue() {
    type_ = PARAMETER_DOUBLE_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::double_array,
                                          ::std::vector<double>(v.begin(), v.end()));
}
inline ParameterValue::ParameterValue(const ::std::vector<double>& v) : ParameterValue() {
    type_ = PARAMETER_DOUBLE_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::double_array, v);
}
inline ParameterValue::ParameterValue(const ::std::vector<::std::string>& v) : ParameterValue() {
    type_ = PARAMETER_STRING_ARRAY;
    heap_ = detail::param_value_heap_with(&detail::ParameterValueHeap::string_array, v);
}

inline const ::std::string& ParameterValue::as_string() const {
    return type_ == PARAMETER_STRING && heap_ ? heap_->string_value
                                              : detail::param_value_empty<::std::string>();
}
inline const ::std::vector<uint8_t>& ParameterValue::as_byte_array() const {
    return type_ == PARAMETER_BYTE_ARRAY && heap_
               ? heap_->byte_array
               : detail::param_value_empty<::std::vector<uint8_t>>();
}
inline const ::std::vector<bool>& ParameterValue::as_bool_array() const {
    return type_ == PARAMETER_BOOL_ARRAY && heap_
               ? heap_->bool_array
               : detail::param_value_empty<::std::vector<bool>>();
}
inline const ::std::vector<int64_t>& ParameterValue::as_integer_array() const {
    return type_ == PARAMETER_INTEGER_ARRAY && heap_
               ? heap_->integer_array
               : detail::param_value_empty<::std::vector<int64_t>>();
}
inline const ::std::vector<double>& ParameterValue::as_double_array() const {
    return type_ == PARAMETER_DOUBLE_ARRAY && heap_
               ? heap_->double_array
               : detail::param_value_empty<::std::vector<double>>();
}
inline const ::std::vector<::std::string>& ParameterValue::as_string_array() const {
    return type_ == PARAMETER_STRING_ARRAY && heap_
               ? heap_->string_array
               : detail::param_value_empty<::std::vector<::std::string>>();
}

inline bool ParameterValue::operator==(const ParameterValue& o) const {
    if (type_ != o.type_) {
        return false;
    }
    switch (type_) {
    case PARAMETER_BOOL:
        return bool_ == o.bool_;
    case PARAMETER_INTEGER:
        return integer_ == o.integer_;
    case PARAMETER_DOUBLE:
        return double_ == o.double_;
    case PARAMETER_STRING:
        return as_string() == o.as_string();
    case PARAMETER_BYTE_ARRAY:
        return as_byte_array() == o.as_byte_array();
    case PARAMETER_BOOL_ARRAY:
        return as_bool_array() == o.as_bool_array();
    case PARAMETER_INTEGER_ARRAY:
        return as_integer_array() == o.as_integer_array();
    case PARAMETER_DOUBLE_ARRAY:
        return as_double_array() == o.as_double_array();
    case PARAMETER_STRING_ARRAY:
        return as_string_array() == o.as_string_array();
    default:
        return true;
    }
}

inline Parameter::Parameter(const ::std::string& name) : Parameter() {
    name_ = new detail::ParameterNameHeap();
    name_->name = name;
}
inline Parameter::Parameter(const ::std::string& name, const ParameterValue& value)
    : Parameter(name) {
    value_ = value;
}

inline Parameter Parameter::from_write(const ParameterWrite& w) {
    ::std::string name = w.name != nullptr ? ::std::string(w.name) : ::std::string();
    switch (w.param_type) {
    case PARAMETER_BOOL:
        return Parameter(name, ParameterValue(w.bool_value));
    case PARAMETER_INTEGER:
        return Parameter(name, ParameterValue(static_cast<long long>(w.integer_value)));
    case PARAMETER_DOUBLE:
        return Parameter(name, ParameterValue(w.double_value));
    case PARAMETER_STRING:
        return Parameter(
            name, ParameterValue(::std::string(w.string_value != nullptr ? w.string_value : "")));
    case PARAMETER_BYTE_ARRAY:
        return Parameter(name, ParameterValue(::std::vector<uint8_t>(
                                   w.byte_array, w.byte_array + (w.byte_array ? w.array_len : 0))));
    case PARAMETER_BOOL_ARRAY:
        return Parameter(name, ParameterValue(::std::vector<bool>(
                                   w.bool_array, w.bool_array + (w.bool_array ? w.array_len : 0))));
    case PARAMETER_INTEGER_ARRAY:
        return Parameter(
            name, ParameterValue(::std::vector<int64_t>(
                      w.integer_array, w.integer_array + (w.integer_array ? w.array_len : 0))));
    case PARAMETER_DOUBLE_ARRAY:
        return Parameter(name,
                         ParameterValue(::std::vector<double>(
                             w.double_array, w.double_array + (w.double_array ? w.array_len : 0))));
    default:
        // A string array: the write carries no elements (see
        // `nros_cpp_param_write_t`), so the callback sees the name and type.
        Parameter p(name);
        p.value_.type_ = w.param_type;
        return p;
    }
}

inline const ::std::string& Parameter::get_name() const {
    return name_ != nullptr ? name_->name : detail::param_value_empty<::std::string>();
}

inline const char* Parameter::get_type_name() const {
    switch (get_type()) {
    case PARAMETER_BOOL:
        return "bool";
    case PARAMETER_INTEGER:
        return "integer";
    case PARAMETER_DOUBLE:
        return "double";
    case PARAMETER_STRING:
        return "string";
    case PARAMETER_BYTE_ARRAY:
        return "byte_array";
    case PARAMETER_BOOL_ARRAY:
        return "bool_array";
    case PARAMETER_INTEGER_ARRAY:
        return "integer_array";
    case PARAMETER_DOUBLE_ARRAY:
        return "double_array";
    case PARAMETER_STRING_ARRAY:
        return "string_array";
    default:
        return "not set";
    }
}

inline bool Parameter::operator==(const Parameter& o) const {
    return get_name() == o.get_name() && value_ == o.value_;
}

#endif // NROS_CPP_STD

} // namespace rclcpp

#endif // NROS_RCLCPP_PARAMETER_VALUE_HPP
