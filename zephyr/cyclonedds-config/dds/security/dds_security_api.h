/* Stub: DDS_HAS_SECURITY=0 on Zephyr; security plugin tree not built.
 * `ddsi_security_omg.h` includes this header at parse time even when
 * all security call sites are guarded out. Pull in the typedef stubs
 * via dds_security_api_types.h so the prototypes in ddsi_security_omg.h
 * parse, plus property-name string constants `ddsi_participant.c` and
 * `ddsi_plist.c` reference unconditionally. */
#ifndef DDS_SECURITY_API_STUB
#define DDS_SECURITY_API_STUB
#include "dds/security/dds_security_api_types.h"

/* Property name string constants — only referenced where security is
 * already runtime-disabled; placeholder values keep the TUs compiling. */
#define DDS_SEC_PROP_PREFIX                      "dds.sec."
#define ORG_ECLIPSE_CYCLONEDDS_SEC_PREFIX        "org.eclipse.cyclonedds.sec."
#define ORG_ECLIPSE_CYCLONEDDS_SEC_AUTH_CRL      "org.eclipse.cyclonedds.sec.auth.crl"
#define DDS_SEC_PROP_AUTH_IDENTITY_CA            "dds.sec.auth.identity_ca"
#define DDS_SEC_PROP_AUTH_PRIV_KEY               "dds.sec.auth.private_key"
#define DDS_SEC_PROP_AUTH_IDENTITY_CERT          "dds.sec.auth.identity_certificate"
#define DDS_SEC_PROP_ACCESS_PERMISSIONS_CA       "dds.sec.access.permissions_ca"
#define DDS_SEC_PROP_ACCESS_GOVERNANCE           "dds.sec.access.governance"
#define DDS_SEC_PROP_ACCESS_PERMISSIONS          "dds.sec.access.permissions"
#define DDS_SEC_PROP_AUTH_LIBRARY_PATH           "dds.sec.auth.library.path"
#define DDS_SEC_PROP_AUTH_LIBRARY_INIT           "dds.sec.auth.library.init"
#define DDS_SEC_PROP_AUTH_LIBRARY_FINALIZE       "dds.sec.auth.library.finalize"
#define DDS_SEC_PROP_CRYPTO_LIBRARY_PATH         "dds.sec.crypto.library.path"
#define DDS_SEC_PROP_CRYPTO_LIBRARY_INIT         "dds.sec.crypto.library.init"
#define DDS_SEC_PROP_CRYPTO_LIBRARY_FINALIZE     "dds.sec.crypto.library.finalize"
#define DDS_SEC_PROP_ACCESS_LIBRARY_PATH         "dds.sec.access.library.path"
#define DDS_SEC_PROP_ACCESS_LIBRARY_INIT         "dds.sec.access.library.init"
#define DDS_SEC_PROP_ACCESS_LIBRARY_FINALIZE     "dds.sec.access.library.finalize"
#define DDS_SEC_PROP_AUTH_PASSWORD               "dds.sec.auth.password"
#define DDS_SEC_PROP_ACCESS_TRUSTED_CA_DIR       "dds.sec.access.trusted_ca_dir"
#endif
