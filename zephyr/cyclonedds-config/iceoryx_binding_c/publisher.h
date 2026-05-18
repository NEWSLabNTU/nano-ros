/* Stub: DDS_HAS_SHM=0 on Zephyr; iceoryx not linked. */
#ifndef ICEORYX_BINDING_C_PUBLISHER_STUB
#define ICEORYX_BINDING_C_PUBLISHER_STUB
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef void * iox_pub_t;
typedef void * iox_allocator_t;

/* Opaque storage — Cyclone allocates one of these per writer.
 * Size doesn't matter under SHM=0; never dereferenced. */
typedef struct { uint64_t _opaque[8]; } iox_pub_storage_t;

typedef struct {
    uint64_t historyCapacity;
    const char * nodeName;
    bool offerDataLossPolicy;
    bool subscriberTooSlowPolicy;
    uint64_t _padding[4];
} iox_pub_options_t;

static inline void iox_pub_options_init(iox_pub_options_t * o) {
    if (o == NULL) return;
    o->historyCapacity = 0;
    o->nodeName = NULL;
    o->offerDataLossPolicy = false;
    o->subscriberTooSlowPolicy = false;
}
static inline iox_pub_t iox_pub_init(iox_pub_storage_t * s, const char * a,
                                     const char * b, const char * c,
                                     const iox_pub_options_t * o) {
    (void)s; (void)a; (void)b; (void)c; (void)o;
    return NULL;
}
#endif
