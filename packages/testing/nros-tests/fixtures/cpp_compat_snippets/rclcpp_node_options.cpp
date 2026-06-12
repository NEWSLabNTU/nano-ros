#include <nros/rclcpp_compat.hpp>

#include <memory>
#include <string>
#include <type_traits>

namespace nros_compat_component_detail {
template <typename T>
typename std::enable_if<std::is_constructible<T, rclcpp::NodeOptions>::value, std::shared_ptr<T>>::type
make_component(const char*) {
    return std::make_shared<T>(rclcpp::NodeOptions{});
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, rclcpp::NodeOptions>::value && std::is_constructible<T>::value, std::shared_ptr<T>>::type
make_component(const char*) {
    return std::make_shared<T>();
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, rclcpp::NodeOptions>::value && !std::is_constructible<T>::value && std::is_constructible<T, const std::string&>::value, std::shared_ptr<T>>::type
make_component(const char* name) {
    return std::make_shared<T>(std::string(name));
}
} // namespace nros_compat_component_detail

class OptionComponent : public rclcpp::Node {
  public:
    explicit OptionComponent(const rclcpp::NodeOptions& options)
        : rclcpp::Node("option_component", options) {}
};

class DefaultComponent : public rclcpp::Node {
  public:
    DefaultComponent() : rclcpp::Node("default_component") {}
};

class NameComponent : public rclcpp::Node {
  public:
    explicit NameComponent(const std::string& name) : rclcpp::Node(name) {}
};

static_assert(std::is_constructible<OptionComponent, rclcpp::NodeOptions>::value,
              "NodeOptions constructor should be available");

int main() {
    auto options = rclcpp::NodeOptions().use_intra_process_comms(true);
    OptionComponent direct(options);
    (void)direct.get_node_options().use_intra_process_comms();
    auto a = nros_compat_component_detail::make_component<OptionComponent>("option");
    auto b = nros_compat_component_detail::make_component<DefaultComponent>("default");
    auto c = nros_compat_component_detail::make_component<NameComponent>("named");
    (void)a;
    (void)b;
    (void)c;
    return 0;
}
