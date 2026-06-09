/* The C node side of the cross-language shared-state example. Includes the
 * bake-generated header and calls the Rust-exported C-ABI accessors; the symbols
 * resolve at the final link against the Rust crate. */
#include "nros_shared_context.h"

void c_write_state(float speed, float heading, uint32_t ticks) {
    VehicleState v = {speed, heading, ticks};
    nros_vehicle_state_set(&v);
}

float c_read_speed(void) {
    VehicleState v;
    nros_vehicle_state_get(&v);
    return v.speed;
}

uint32_t c_read_ticks(void) {
    VehicleState v;
    nros_vehicle_state_get(&v);
    return v.ticks;
}

/* Guarded read-modify-write from C through the Rust `modify` accessor — the
 * closure runs under the region's `critical_section` lock. */
static void c_bump(VehicleState *s, void *ctx) {
    (void)ctx;
    s->ticks += 1;
}

void c_modify_bump(void) { nros_vehicle_state_modify(c_bump, 0); }
