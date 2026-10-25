// SPDX-License-Identifier: Apache-2.0
// <rclcpp_components/register_node_macro.hpp>
//
// nano-ros is single-binary: there is no runtime ComponentManager and no
// class-loader plugin index. Composition is modelled at build time instead:
// `rclcpp_components_register_node(<lib> PLUGIN <class> EXECUTABLE <bin>)` in
// `cmake/NanoRosAmentSurface.cmake` synthesises a `main()` that constructs the
// registered class and spins it.
//
// That `main()` cannot include the class's header: upstream's never does (it
// loads the class through class_loader), so a stock CMakeLists names nothing
// but the class's spelling. So the factory lives HERE, in the component's own
// translation unit where the class is complete: the macro defines an explicit
// specialization of `make_component<NodeClass>`, and the generated `main()`
// forward-declares the class and declares the same specialization. The linker
// joins them, and the reference also keeps the component's object in a static
// library. Phase-488 (porting `autoware_external_cmd_selector`).
//
// A missing `RCLCPP_COMPONENTS_REGISTER_NODE(...)` for a class an EXECUTABLE
// names is a link error on `make_component<...>`.
#ifndef NROS_RCLCPP_COMPONENTS_REGISTER_NODE_MACRO_HPP
#define NROS_RCLCPP_COMPONENTS_REGISTER_NODE_MACRO_HPP

// Components exist only on the std-shaped surface (the ament verbs ask for it),
// and the factory returns a `std::shared_ptr`.
#include "rclcpp/detail/std_detect.hpp"

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
// `<memory>` and `<string>` come from `std_detect.hpp`, which defines the
// capability only after including them.
#include <type_traits>

#include <rclcpp/rclcpp.hpp>

namespace nros_ament_component_detail {

// Defined once per registered class by RCLCPP_COMPONENTS_REGISTER_NODE below.
template <typename T>
std::shared_ptr<rclcpp::Node> make_component(const rclcpp::NodeOptions& options, const char* name);

// Prefer the stock component constructor `T(const rclcpp::NodeOptions&)`, then
// the older nano-ros smoke shapes `T()` and `T(std::string)`.
template <typename T>
typename std::enable_if<std::is_constructible<T, const rclcpp::NodeOptions&>::value,
                        std::shared_ptr<rclcpp::Node>>::type
construct(const rclcpp::NodeOptions& options, const char*) {
    return std::make_shared<T>(options);
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, const rclcpp::NodeOptions&>::value &&
                            std::is_constructible<T>::value,
                        std::shared_ptr<rclcpp::Node>>::type
construct(const rclcpp::NodeOptions&, const char*) {
    return std::make_shared<T>();
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, const rclcpp::NodeOptions&>::value &&
                            !std::is_constructible<T>::value &&
                            std::is_constructible<T, const std::string&>::value,
                        std::shared_ptr<rclcpp::Node>>::type
construct(const rclcpp::NodeOptions&, const char* name) {
    return std::make_shared<T>(std::string(name));
}

} // namespace nros_ament_component_detail

#ifndef RCLCPP_COMPONENTS_REGISTER_NODE
#define RCLCPP_COMPONENTS_REGISTER_NODE(NodeClass)                                                 \
    template <>                                                                                    \
    std::shared_ptr<rclcpp::Node> nros_ament_component_detail::make_component<NodeClass>(          \
        const rclcpp::NodeOptions& options, const char* name) {                                    \
        return nros_ament_component_detail::construct<NodeClass>(options, name);                   \
    }
#endif

#else // freestanding: no allocator for a component, and no generated main.

#ifndef RCLCPP_COMPONENTS_REGISTER_NODE
#define RCLCPP_COMPONENTS_REGISTER_NODE(NodeClass) /* no-op (freestanding) */
#endif

#endif
#endif
