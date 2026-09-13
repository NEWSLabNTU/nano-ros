// POSITIVE compile probe — phase-417 W4.a's C++ parameter surface, on the
// UNCONDITIONAL half of `rclcpp::Node`.
//
// The claim this file makes is the one W4.a exists for: a ported rclcpp node
// can name `undeclare_parameter`, `describe_parameter`, `get_parameter_type`,
// `get_parameter_types`, `list_parameters`, `set_parameters_atomically`,
// `get_parameter_or`, `add_on_set_parameters_callback`,
// `remove_on_set_parameters_callback`, `rclcpp::ParameterType`,
// `rclcpp::ParameterDescriptor` and the descriptor-carrying
// `declare_parameter<T>` — and can do it on a FREESTANDING target, because
// none of it needs the STL.
//
// It is compiled in BOTH arms the `rclcpp_node_freestanding_surface.cpp` probe
// uses: hosted, and `-nostdinc++` against the ThreadX shim. That second arm is
// the one that catches an `#include <string>` or an `std::` creeping into
// `node_parameters.hpp` — the hazard issue 0332 records, and the reason this
// surface deliberately has no descriptor STRUCT with an inline text buffer.
//
// `NROS_CPP_STD` is NOT defined here, on purpose: everything below must live on
// the half a `-nostdinc++` board gets. The hosted-only additions
// (`std::string` keys, `declare_parameters` over a `std::map`) belong in
// `param_hosted_overloads.cpp`, which defines the macro.
//
// A `-fsyntax-only` probe rather than a run: the methods forward to the FFI,
// and what the STORE does with them is measured in `nros-params`' unit tests
// and in `executor_param_descriptors.c`. What can only be checked here is that
// the DECLARATIONS instantiate at all, and on a target with no STL.

#include <nros/nros.hpp>

namespace nros_cpp_param_descriptor_surface_test {

// A hook has to be a plain function with C linkage; there is no allocator to
// hold a closure and the freestanding lane has no `<functional>`. This is the
// shape a ported `add_on_set_parameters_callback` argument becomes.
extern "C" bool refuse_negative_rate(const rclcpp::ParameterWrite* write, void* context) {
    (void)context;
    if (write == nullptr) {
        return false;
    }
    if (write->param_type == rclcpp::PARAMETER_DOUBLE && write->double_value < 0.0) {
        return false;
    }
    // The string arm reads the borrowed buffer, which is valid for the call.
    if (write->param_type == rclcpp::PARAMETER_STRING && write->string_value == nullptr) {
        return false;
    }
    return true;
}

inline void instantiate(rclcpp::Node& node) {
    // The enum is NAMED, under upstream's own spellings, both qualified and
    // bare — rclcpp's is a plain enum, so both resolve there and both must
    // resolve here.
    rclcpp::ParameterType qualified = rclcpp::ParameterType::PARAMETER_DOUBLE;
    rclcpp::ParameterType bare = rclcpp::PARAMETER_INTEGER;
    (void)qualified;
    (void)bare;

    // declare with a descriptor: upstream's three-argument form, with the
    // descriptor's text BORROWED (string literals, no allocator).
    rclcpp::ParameterDescriptor d = rclcpp::parameter_descriptor();
    d.description = "publish rate";
    d.additional_constraints = "hz, positive";
    d.has_range = true;
    d.double_from = 0.0;
    d.double_to = 100.0;
    d.double_step = 0.0;
    double rate = node.declare_parameter<double>("rate", 2.0, d);
    (void)rate;

    // read with a caller-chosen fallback, where `get_parameter<T>` falls back
    // to `T()`.
    double with_fallback = 0.0;
    bool found = node.get_parameter_or<double>("rate", with_fallback, 9.0);
    (void)found;

    // the type query, singular and plural
    rclcpp::ParameterType t = node.get_parameter_type("rate");
    (void)t;
    const char* names[2] = {"rate", "depth"};
    rclcpp::ParameterType types[2] = {rclcpp::PARAMETER_NOT_SET, rclcpp::PARAMETER_NOT_SET};
    nros::Result r = node.get_parameter_types(names, 2, types);
    (void)r.ok();

    // describe into caller storage — one buffer, split between the two texts.
    char text[128];
    rclcpp::ParameterDescriptor read_back = rclcpp::parameter_descriptor();
    r = node.describe_parameter("rate", read_back, text, sizeof(text));
    (void)read_back.read_only;
    (void)read_back.has_range;
    (void)read_back.description;
    (void)read_back.additional_constraints;

    // list into a caller-owned rectangle; `max_names == 0` counts.
    char rows[4][32];
    ::size_t count = 0;
    r = node.list_parameters("", &rows[0][0], sizeof(rows[0]), 4, count);
    r = node.list_parameters(nullptr, nullptr, 0, 0, count);

    // the atomic multi-set, over the same value type the hook sees
    rclcpp::ParameterWrite writes[2];
    writes[0].name = "rate";
    writes[0].param_type = rclcpp::PARAMETER_DOUBLE;
    writes[0].bool_value = false;
    writes[0].integer_value = 0;
    writes[0].double_value = 5.0;
    writes[0].string_value = nullptr;
    writes[1] = writes[0];
    writes[1].name = "depth";
    writes[1].param_type = rclcpp::PARAMETER_INTEGER;
    writes[1].integer_value = 10;
    r = node.set_parameters_atomically(writes, 2);

    // the accept/reject hook, register and unregister
    rclcpp::ParameterCallbackHandle handle = 0;
    r = node.add_on_set_parameters_callback(refuse_negative_rate, nullptr, handle);
    r = node.remove_on_set_parameters_callback(handle);

    // undeclare
    r = node.undeclare_parameter("depth");
    (void)r.ok();
}

// Reference it so the template bodies are instantiated rather than merely
// parsed: a `-fsyntax-only` that never names the function would type-check the
// declarations and skip every definition, which is the shape that passes on
// absence.
inline void (*keep)(rclcpp::Node&) = &instantiate;

} // namespace nros_cpp_param_descriptor_surface_test
