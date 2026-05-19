#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#define ZID_PRESERVE_BACKWARDS_COMPAT 1
#include <zenoh-pico.h>
#include <zenoh-pico/net/publish.h>
#include <zenoh-pico/net/subscribe.h>
#include <zenoh-pico/net/query.h>
#include <zenoh-pico/net/session.h>
#include <zenoh-pico/session/session.h>
#include <zenoh-pico/transport/transport.h>

int main(void) {
    printf("=== zenoh-pico internal storage (heap-allocated per entity) ===\n");
    printf("_z_publisher_t                 %zu B\n", sizeof(_z_publisher_t));
    printf("_z_subscriber_t                %zu B\n", sizeof(_z_subscriber_t));
    printf("_z_queryable_t                 %zu B\n", sizeof(_z_queryable_t));
    printf("_z_session_t                   %zu B\n", sizeof(_z_session_t));
    printf("_z_transport_t                 %zu B\n", sizeof(_z_transport_t));
    return 0;
}
