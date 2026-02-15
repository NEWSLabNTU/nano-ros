/* Helper functions for Rust FFI that require field access to opaque C structs. */

#include <uxr/client/core/session/session.h>
#include <uxr/client/profile/transport/custom/custom_transport.h>

/* Return a pointer to the embedded uxrCommunication within a uxrCustomTransport.
 * Needed because the Rust side uses opaque types and cannot access struct fields. */
uxrCommunication* uxr_custom_transport_comm(uxrCustomTransport* transport)
{
    return &transport->comm;
}
