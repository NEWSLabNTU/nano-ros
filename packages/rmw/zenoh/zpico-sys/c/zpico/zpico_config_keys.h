/* GENERATED FILE — DO NOT EDIT.
 *
 * Regenerate with:  python3 scripts/gen-zpico-config-keys.py
 * Staleness gate:   just check zpico-config-keys
 *
 * Source of truth:
 *   packages/rmw/zenoh/zpico-sys/zenoh-pico/include/zenoh-pico/config.h
 *
 * zenoh-pico's run-time options ARE its configuration surface — there is no
 * config-file format for the pico client. This table maps the nano-ros
 * property name a caller writes onto the `Z_CONFIG_*_KEY` constant
 * `zp_config_insert()` takes. 23 keys are derived mechanically
 * (`Z_CONFIG_<X>_KEY` -> `lowercase(<X>)`); 4 legacy aliases are
 * authored in the generator.
 *
 * `needs_tls` marks the keys that only mean anything when zenoh-pico is built
 * with `Z_FEATURE_LINK_TLS`. They stay in the table on every build so that
 * supplying one to a build WITHOUT TLS is a reported error rather than a
 * silent no-op.
 */

#ifndef ZPICO_CONFIG_KEYS_H
#define ZPICO_CONFIG_KEYS_H

#include <stdbool.h>
#include <stdint.h>

/* Requires <zenoh-pico.h> (for the Z_CONFIG_*_KEY macros) to be included
 * first; this header deliberately does not include it, so it can be parsed by
 * the coverage test without a zenoh-pico include path. */

typedef struct zpico_config_key_entry {
    const char* name; /* nano-ros property name */
    uint8_t key;      /* Z_CONFIG_*_KEY */
    bool needs_tls;   /* meaningful only with Z_FEATURE_LINK_TLS */
} zpico_config_key_entry;

static const zpico_config_key_entry ZPICO_CONFIG_KEYS[] = {
    {"mode", Z_CONFIG_MODE_KEY, false},
    {"connect", Z_CONFIG_CONNECT_KEY, false},
    {"listen", Z_CONFIG_LISTEN_KEY, false},
    {"user", Z_CONFIG_USER_KEY, false},
    {"password", Z_CONFIG_PASSWORD_KEY, false},
    {"multicast_scouting", Z_CONFIG_MULTICAST_SCOUTING_KEY, false},
    {"multicast_locator", Z_CONFIG_MULTICAST_LOCATOR_KEY, false},
    {"scouting_timeout", Z_CONFIG_SCOUTING_TIMEOUT_KEY, false},
    {"scouting_what", Z_CONFIG_SCOUTING_WHAT_KEY, false},
    {"session_zid", Z_CONFIG_SESSION_ZID_KEY, false},
    {"add_timestamp", Z_CONFIG_ADD_TIMESTAMP_KEY, false},
    {"tls_root_ca_certificate", Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_KEY, true},
    {"tls_root_ca_certificate_base64", Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_BASE64_KEY, true},
    {"tls_listen_private_key", Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_KEY, true},
    {"tls_listen_private_key_base64", Z_CONFIG_TLS_LISTEN_PRIVATE_KEY_BASE64_KEY, true},
    {"tls_listen_certificate", Z_CONFIG_TLS_LISTEN_CERTIFICATE_KEY, true},
    {"tls_listen_certificate_base64", Z_CONFIG_TLS_LISTEN_CERTIFICATE_BASE64_KEY, true},
    {"tls_enable_mtls", Z_CONFIG_TLS_ENABLE_MTLS_KEY, true},
    {"tls_connect_private_key", Z_CONFIG_TLS_CONNECT_PRIVATE_KEY_KEY, true},
    {"tls_connect_private_key_base64", Z_CONFIG_TLS_CONNECT_PRIVATE_KEY_BASE64_KEY, true},
    {"tls_connect_certificate", Z_CONFIG_TLS_CONNECT_CERTIFICATE_KEY, true},
    {"tls_connect_certificate_base64", Z_CONFIG_TLS_CONNECT_CERTIFICATE_BASE64_KEY, true},
    {"tls_verify_name_on_connect", Z_CONFIG_TLS_VERIFY_NAME_ON_CONNECT_KEY, true},
    {"root_ca_certificate", Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_KEY, true},
    {"root_ca_certificate_base64", Z_CONFIG_TLS_ROOT_CA_CERTIFICATE_BASE64_KEY, true},
    {"scouting_timeout_ms", Z_CONFIG_SCOUTING_TIMEOUT_KEY, false},
    {"verify_name_on_connect", Z_CONFIG_TLS_VERIFY_NAME_ON_CONNECT_KEY, true},
};

#define ZPICO_CONFIG_KEY_COUNT (sizeof(ZPICO_CONFIG_KEYS) / sizeof(ZPICO_CONFIG_KEYS[0]))

#endif /* ZPICO_CONFIG_KEYS_H */
