// phase-482 W2 — EXPECTED TO FAIL TO COMPILE.
//
// `rclcpp::Node::SharedPtr` is `nros::Handle<Node>`, which observes and does
// not own. Initialising one from a TEMPORARY `std::shared_ptr` would leave it
// dangling at the end of the statement, where upstream's `shared_ptr` would
// have kept the node alive. So the conversion from an rvalue is deleted, and
// this upstream spelling is a compile error naming the deleted constructor
// (RFC-0096 D5). The fix is `auto node = std::make_shared<rclcpp::Node>(...)`.
//
// `just check cpp` compiles this and requires it to FAIL with a `deleted`
// diagnostic; compiling cleanly would mean a handle can dangle in silence.

#include <memory>

#include <nros/nros.hpp>

void dangling_handle() {
    rclcpp::Node::SharedPtr node = std::make_shared<rclcpp::Node>("dangles");
    (void)node;
}
