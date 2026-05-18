/*
 * Stub for DDS_HAS_SECURITY=0 on Zephyr. Declares the opaque
 * handle/attribute typedefs that `ddsi_security_omg.h` references
 * in function prototypes — even with all security call sites
 * guarded out, the prototypes must parse.
 *
 * Real definitions live in the security plugin tree
 * (`src/security/api/include/dds/security/...`) which the Zephyr
 * build skips. Under SECURITY=0 these types are never referenced
 * at runtime, so opaque integer typedefs are sufficient.
 */
#ifndef DDS_SECURITY_API_TYPES_STUB
#define DDS_SECURITY_API_TYPES_STUB

#include <stdint.h>

typedef int64_t  DDS_Security_long_long;
typedef uint64_t DDS_Security_unsigned_long_long;
typedef int32_t  DDS_Security_long;
typedef uint32_t DDS_Security_unsigned_long;

/* Opaque handles — long long per OMG DDS Security spec. */
typedef int64_t DDS_Security_IdentityHandle;
typedef int64_t DDS_Security_PermissionsHandle;
typedef int64_t DDS_Security_ParticipantCryptoHandle;
typedef int64_t DDS_Security_DatawriterCryptoHandle;
typedef int64_t DDS_Security_DatareaderCryptoHandle;
typedef int64_t DDS_Security_SharedSecretHandle;
typedef int64_t DDS_Security_InstanceHandle;

/* Attribute / exception value-types — opaque structs. */
typedef struct { int _unused; } DDS_Security_ParticipantSecurityAttributes;
typedef struct { int _unused; } DDS_Security_EndpointSecurityAttributes;
typedef struct { int _unused; } DDS_Security_SecurityException;
typedef struct { int _unused; } DDS_Security_PropertyQosPolicy;
typedef struct { int _unused; } DDS_Security_DataHolderSeq;
typedef struct { int _unused; } DDS_Security_BinaryProperty_t;
typedef struct { int _unused; } DDS_Security_OctetSeq;

#endif
