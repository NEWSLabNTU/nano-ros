// nros-cpp: upstream's request header, the ONE C++ spelling.
// Freestanding C++ -- no exceptions, no STL required.

/**
 * @file rmw_request_id.hpp
 * @ingroup grp_service
 * @brief `rmw_request_id_t` — upstream's request header (`<rmw/types.h>`).
 *
 * Both service halves hand it out: the three-argument service handler
 * (`rclcpp/service.hpp`, issue 2081) and `PollClient<S>::take_response`
 * (`rclcpp/polling_client.hpp`, issue 2148). One header, so the two cannot
 * drift apart.
 */

#ifndef NROS_RCLCPP_DETAIL_RMW_REQUEST_ID_HPP
#define NROS_RCLCPP_DETAIL_RMW_REQUEST_ID_HPP

#include <cstdint>

#ifndef NROS_RMW_REQUEST_ID_T_DEFINED
#define NROS_RMW_REQUEST_ID_T_DEFINED
/// `rmw_request_id_t` — who sent a service request (the client's writer GUID)
/// and the sequence number it assigned. A backend that reports no GUID leaves
/// `writer_guid` all zero. The same struct, under the same guard, as
/// `<nros/rmw_entity.h>` and `<rclc/detail/request_id.h>`.
extern "C" {
typedef struct rmw_request_id_t {
    int8_t writer_guid[16];
    int64_t sequence_number;
} rmw_request_id_t;
} // extern "C"
#endif
static_assert(sizeof(rmw_request_id_t) == 24, "rmw_request_id_t: upstream's 16 + 8 bytes");

#endif // NROS_RCLCPP_DETAIL_RMW_REQUEST_ID_HPP
