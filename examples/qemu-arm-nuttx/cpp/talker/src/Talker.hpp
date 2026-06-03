// Phase 212.L Component pkg — NuttX C++ talker.
#ifndef NUTTX_CPP_TALKER_TALKER_HPP
#define NUTTX_CPP_TALKER_TALKER_HPP

#include <nros/node_pkg.hpp>

namespace nuttx_cpp_talker {

class Talker {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace nuttx_cpp_talker

#endif // NUTTX_CPP_TALKER_TALKER_HPP
