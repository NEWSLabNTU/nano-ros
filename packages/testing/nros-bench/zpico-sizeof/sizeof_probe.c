#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <zenoh-pico.h>

typedef struct {
    z_owned_publisher_t publisher;
    bool active;
} publisher_entry_t;

typedef struct {
    z_owned_liveliness_token_t token;
    bool active;
} liveliness_entry_t;

typedef struct {
    z_owned_queryable_t queryable;
    void *callback;
    void *ctx;
    bool active;
} queryable_entry_t;

typedef struct {
    z_owned_subscriber_t subscriber;
    union { void *cb1; void *cb2; void *cb3; };
    void *ctx;
    bool active; bool with_attachment;
    bool direct_write;
    uint8_t *buf_ptr; size_t buf_capacity;
    const bool *locked_ptr;
    bool ring_mode;
    void *ring;
} subscriber_entry_t;

int main(void) {
    printf("=== zenoh-pico opaque sizes (posix build) ===\n");
    printf("z_owned_session_t              %zu B\n", sizeof(z_owned_session_t));
    printf("z_owned_publisher_t            %zu B\n", sizeof(z_owned_publisher_t));
    printf("z_owned_subscriber_t           %zu B\n", sizeof(z_owned_subscriber_t));
    printf("z_owned_queryable_t            %zu B\n", sizeof(z_owned_queryable_t));
    printf("z_owned_liveliness_token_t     %zu B\n", sizeof(z_owned_liveliness_token_t));
    printf("z_owned_query_t                %zu B\n", sizeof(z_owned_query_t));
    printf("z_owned_config_t               %zu B\n", sizeof(z_owned_config_t));
    printf("\n=== zpico entry-table per-slot overhead ===\n");
    printf("publisher_entry_t              %zu B (×ZPICO_MAX_PUBLISHERS)\n", sizeof(publisher_entry_t));
    printf("subscriber_entry_t             %zu B (×ZPICO_MAX_SUBSCRIBERS)\n", sizeof(subscriber_entry_t));
    printf("queryable_entry_t              %zu B (×ZPICO_MAX_QUERYABLES)\n", sizeof(queryable_entry_t));
    printf("liveliness_entry_t             %zu B (×ZPICO_MAX_LIVELINESS)\n", sizeof(liveliness_entry_t));
    printf("g_stored_query slot            %zu B (×ZPICO_MAX_QUERYABLES)\n", sizeof(z_owned_query_t) + sizeof(bool));
    return 0;
}
