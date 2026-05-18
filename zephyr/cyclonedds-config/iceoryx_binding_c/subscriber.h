/* Stub: DDS_HAS_SHM=0 on Zephyr; iceoryx not linked. */
#ifndef ICEORYX_BINDING_C_SUBSCRIBER_STUB
#define ICEORYX_BINDING_C_SUBSCRIBER_STUB
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef void * iox_sub_t;
typedef void * iox_user_trigger_t;
typedef void * iox_listener_t;
typedef void * iox_notification_info_t;
typedef void * iox_sub_context_t;

/* Opaque storage — Cyclone allocates one of these per reader. */
typedef struct { uint64_t _opaque[8]; } iox_sub_storage_t;

typedef struct {
    uint64_t queueCapacity;
    uint64_t historyRequest;
    bool requirePublisherHistorySupport;
    const char * nodeName;
    uint64_t _padding[4];
} iox_sub_options_t;

static inline void iox_sub_options_init(iox_sub_options_t * o) {
    if (o == NULL) return;
    o->queueCapacity = 0;
    o->historyRequest = 0;
    o->requirePublisherHistorySupport = false;
    o->nodeName = NULL;
}
static inline iox_sub_t iox_sub_init(iox_sub_storage_t * s, const char * a,
                                     const char * b, const char * c,
                                     const iox_sub_options_t * o) {
    (void)s; (void)a; (void)b; (void)c; (void)o;
    return NULL;
}
#endif
