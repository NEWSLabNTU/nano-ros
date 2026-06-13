// ThreadX-Linux C++ AddTwoInts service server — typed component (RFC-0043).
// `configure` binds the member `handle_add` (by identity) as a raw callback-style
// service on `/add_two_ints`; the handler decodes the CDR request (int64 a, b) and
// writes the CDR reply (int64 sum). No interpreter, no callback name.
#ifndef THREADX_LINUX_CPP_SERVICE_SERVER_SERVICESERVER_HPP
#define THREADX_LINUX_CPP_SERVICE_SERVER_SERVICESERVER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace threadx_linux_cpp_service_server {

class ServiceServer {
    bool handle_add(const uint8_t* req, size_t req_len, uint8_t* resp, size_t resp_cap,
                    size_t* resp_len);

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_service_server

#endif
