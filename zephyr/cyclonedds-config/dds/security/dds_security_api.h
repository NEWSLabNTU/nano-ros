/* Stub: DDS_HAS_SECURITY=0 on Zephyr; security plugin tree not built.
 * `ddsi_security_omg.h` includes this header at parse time even when
 * all security call sites are guarded out. Pre-bake an empty stub so
 * the include succeeds; nothing in the no-security build path uses
 * the symbols this header would declare. */
#ifndef DDS_SECURITY_API_STUB
#define DDS_SECURITY_API_STUB
#endif
