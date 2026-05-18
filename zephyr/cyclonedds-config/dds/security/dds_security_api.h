/* Stub: DDS_HAS_SECURITY=0 on Zephyr; security plugin tree not built.
 * `ddsi_security_omg.h` includes this header at parse time even when
 * all security call sites are guarded out. Pull in the typedef stubs
 * via dds_security_api_types.h so the prototypes in ddsi_security_omg.h
 * parse. */
#ifndef DDS_SECURITY_API_STUB
#define DDS_SECURITY_API_STUB
#include "dds/security/dds_security_api_types.h"
#endif
