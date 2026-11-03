/// @file Witness.c
/// @brief phase-521 W7 (RFC-0113 D2) -- the idle-wake witness for Zephyr.
///
/// One subscription nobody publishes to and one 1 s timer. RFC-0113 D2 says an
/// idle executor wakes only for its deadlines and for arrivals, so over a
/// window of WINDOW_FIRES timer periods the thread that runs the executor
/// should be switched in about once per expiry. The count comes from
/// Zephyr's user tracing hook (`CONFIG_TRACING_USER`): every switch-in of that
/// thread is one wake. A 10 ms slice (the wait this phase deleted) would show
/// ~100 a second.
///
/// The window opens at the first expiry, once the session is up, and closes
/// WINDOW_FIRES expiries later; the result is one line the test parses:
///   idle-wake: switch_ins=N fires=F window_ms=W max_late_us=L max_early_us=E

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <zephyr/kernel.h>

#include <rclc/rclc.h>

#include <std_msgs/msg/string.h>

#define PERIOD_MS 1000
#define WINDOW_FIRES 5

typedef struct {
    rcl_subscription_t sub;
    rcl_timer_t timer;
    std_msgs__msg__String msg;
    std_msgs__msg__String__Storage msg_storage;
    int fires;
    int64_t first_us;
    int64_t max_late_us;
    int64_t max_early_us;
} witness_t;

/* The executor's thread, learned from the first timer dispatch, and the
 * switch-in count of that thread while the window is open. Written from the
 * scheduler's switch-in hook, so plain volatiles: the hook runs with
 * interrupts locked on a single-core Cortex-M3. */
static struct k_thread* volatile exec_thread;
static volatile uint32_t switch_ins;
static volatile bool window_open;

void sys_trace_thread_switched_in_user(void) {
    if (window_open && k_current_get() == exec_thread) {
        switch_ins++;
    }
}

static int64_t now_us(void) {
    return (int64_t)k_ticks_to_us_floor64(k_uptime_ticks());
}

static void on_msg(const void* msgin, void* ctx) {
    (void)ctx;
    const std_msgs__msg__String* msg = (const std_msgs__msg__String*)msgin;
    printf("idle-wake: unexpected sample [%s]\n", msg->data.data);
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    witness_t* self = RCLC_CONTAINER_OF(timer, witness_t, timer);
    (void)last_call_time;
    int64_t t = now_us();
    if (self->fires == 0) {
        exec_thread = k_current_get();
        self->first_us = t;
        switch_ins = 0;
        window_open = true;
    } else if (self->fires <= WINDOW_FIRES) {
        int64_t due = self->first_us + (int64_t)self->fires * PERIOD_MS * 1000;
        if (t > due && t - due > self->max_late_us) {
            self->max_late_us = t - due;
        }
        if (t < due && due - t > self->max_early_us) {
            self->max_early_us = due - t;
        }
    }
    if (self->fires == WINDOW_FIRES) {
        window_open = false;
        printf("idle-wake: switch_ins=%u fires=%d window_ms=%d max_late_us=%lld "
               "max_early_us=%lld\n",
               (unsigned)switch_ins, WINDOW_FIRES, (int)((t - self->first_us) / 1000),
               (long long)self->max_late_us, (long long)self->max_early_us);
    }
    self->fires++;
}

static rcl_ret_t witness_configure(rcl_node_t* node, rclc_executor_t* executor, witness_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->fires = 0;
    self->max_late_us = 0;
    self->max_early_us = 0;
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/idle_wake");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                     ON_NEW_DATA);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, (uint64_t)PERIOD_MS * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_timer(executor, &self->timer);
    if (rc == RCL_RET_OK) {
        printf("idle-wake: armed\n");
    }
    return rc;
}

RCLC_C_COMPONENT(witness_t, witness_configure)
