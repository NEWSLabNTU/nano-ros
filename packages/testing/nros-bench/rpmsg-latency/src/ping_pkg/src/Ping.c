/*
 * The timing half of the RPMsg round-trip bench: publish a sequence number
 * on /ping, take it back on /pong (the other core echoes it), and time the
 * round trip with this core's cycle counter, the only clock both ends of a
 * round trip share. One ping is in flight at a time, so a sample is one
 * round trip and never a queue. After NROS_BENCH_SAMPLES round trips it
 * prints the distribution, in microseconds, on one line a harness can parse:
 *
 *   rtt_us: n=<n> min=<v> p50=<v> p90=<v> p99=<v> max=<v> lost=<n>
 *
 * A one-way hop is half the round trip only if the two directions cost the
 * same; on one link with one framing they are the same path.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

/* The clock: the core's cycle counter on Zephyr, where this bench runs; a
 * monotonic clock elsewhere, so the component still compiles for the host
 * metadata probe. */
#if defined(__ZEPHYR__)
#include <zephyr/kernel.h>
static uint32_t bench_cycles(void) { return k_cycle_get_32(); }
static uint64_t bench_cycles_per_sec(void) { return sys_clock_hw_cycles_per_sec(); }
static int64_t bench_ms(void) { return k_uptime_get(); }
#else
#include <time.h>
static uint32_t bench_cycles(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)((uint64_t)ts.tv_sec * 1000000000u + (uint64_t)ts.tv_nsec);
}
static uint64_t bench_cycles_per_sec(void) { return 1000000000u; }
static int64_t bench_ms(void) { return (int64_t)(bench_cycles() / 1000000u); }
#endif

#ifndef NROS_BENCH_SAMPLES
#define NROS_BENCH_SAMPLES 500
#endif
#if NROS_BENCH_SAMPLES < 1
#error "NROS_BENCH_SAMPLES must be >= 1: it sizes a C array"
#endif
/* Samples discarded first: the session's own start-up traffic. */
#define NROS_BENCH_WARMUP 20
/* A ping unanswered this long is counted lost and the next one goes. */
#define NROS_BENCH_TIMEOUT_MS 500

typedef struct {
    rcl_publisher_t pub;
    rcl_subscription_t sub;
    rcl_timer_t timer;
    std_msgs__msg__UInt32 out;
    std_msgs__msg__UInt32 in;
    uint32_t seq;
    bool in_flight;
    uint32_t sent_cycles;
    int64_t sent_ms;
    uint32_t lost;
    uint32_t warm;
    uint32_t n;
    bool done;
    uint32_t rtt_us[NROS_BENCH_SAMPLES];
} ping_t;

static int cmp_u32(const void* a, const void* b) {
    uint32_t x = *(const uint32_t*)a;
    uint32_t y = *(const uint32_t*)b;
    return (x > y) - (x < y);
}

static uint32_t pct(const uint32_t* sorted, uint32_t n, uint32_t p) {
    /* Nearest rank. */
    uint32_t rank = (p * n + 99) / 100;
    return sorted[rank == 0 ? 0 : rank - 1];
}

static void report(ping_t* self) {
    qsort(self->rtt_us, self->n, sizeof(self->rtt_us[0]), cmp_u32);
    printf("rtt_us: n=%u min=%u p50=%u p90=%u p99=%u max=%u lost=%u\n", (unsigned)self->n,
           (unsigned)self->rtt_us[0], (unsigned)pct(self->rtt_us, self->n, 50),
           (unsigned)pct(self->rtt_us, self->n, 90), (unsigned)pct(self->rtt_us, self->n, 99),
           (unsigned)self->rtt_us[self->n - 1], (unsigned)self->lost);
}

static void send_ping(ping_t* self) {
    self->seq++;
    self->out.data = self->seq;
    self->sent_ms = bench_ms();
    self->sent_cycles = bench_cycles();
    if (rcl_publish(&self->pub, &self->out, NULL) == RCL_RET_OK) {
        self->in_flight = true;
    }
}

static void on_pong(const void* msgin, void* ctx) {
    uint32_t now = bench_cycles();
    ping_t* self = (ping_t*)ctx;
    const std_msgs__msg__UInt32* msg = (const std_msgs__msg__UInt32*)msgin;
    if (self->done || !self->in_flight || msg->data != self->seq) {
        return; /* a late answer to a ping already counted lost */
    }
    self->in_flight = false;
    uint64_t cycles = (uint32_t)(now - self->sent_cycles);
    uint32_t us = (uint32_t)(cycles * 1000000u / bench_cycles_per_sec());
    if (self->warm < NROS_BENCH_WARMUP) {
        self->warm++;
    } else {
        self->rtt_us[self->n++] = us;
    }
    if (self->n == NROS_BENCH_SAMPLES) {
        self->done = true;
        report(self);
        return;
    }
    send_ping(self);
}

/* Starts the exchange, and restarts it after a lost ping. */
static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    ping_t* self = RCLC_CONTAINER_OF(timer, ping_t, timer);
    (void)last_call_time;
    if (self->done) {
        return;
    }
    if (self->in_flight) {
        if (bench_ms() - self->sent_ms < NROS_BENCH_TIMEOUT_MS) {
            return;
        }
        self->lost++;
        self->in_flight = false;
    }
    send_ping(self);
}

static rcl_ret_t ping_configure(rcl_node_t* node, rclc_executor_t* executor, ping_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->seq = 0;
    self->in_flight = false;
    self->lost = 0;
    self->warm = 0;
    self->n = 0;
    self->done = false;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, UInt32), "/ping");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->sub = rcl_get_zero_initialized_subscription();
    rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, UInt32), "/pong");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->in, on_pong,
                                                     self, ON_NEW_DATA);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 100ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    printf("ping: %u round trips after %u warm-up\n", (unsigned)NROS_BENCH_SAMPLES,
           (unsigned)NROS_BENCH_WARMUP);
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(ping_t, ping_configure)
