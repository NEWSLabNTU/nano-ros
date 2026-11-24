/*
 * Issue 2878 -- an out-of-tree tracing backend, selected by Kconfig.
 *
 * `nros_test_ring` is registered here with TRACING_BACKEND_DEFINE() and named
 * by CONFIG_TRACING_BACKEND_CUSTOM_NAME (prj.conf). It records every byte
 * the CTF formatter hands it, linearly, until full. main() makes some
 * scheduling traffic, renames its own thread (a stock CTF event,
 * `thread_name_set`, on both lines: the thread id and its name), stops
 * recording, and prints what the backend holds:
 *
 *   tracing-backend: <name>      the name tracing_core.c looked up
 *   tracing-init-calls: <n>      the backend's init() ran (1)
 *   tracing-thread: <hex>        the main thread's CTF id (its address, u32)
 *   tracing-bytes: <n>           bytes the backend received
 *   tracing-hex: <hex>           the recorded stream, 32 bytes per line
 *   tracing-done
 *
 * The driver (just zephyr test-tracing-custom-backend) finds that event's
 * payload (thread id, then the name) in the stream: the CTF stream went
 * through this backend, not an in-tree one.
 */

#include <zephyr/kernel.h>
#include <zephyr/sys/printk.h>
#include <tracing_backend.h>

#include <stdint.h>
#include <string.h>

#define THREAD_NAME "nros-2878"

static uint8_t ring[8192];
static size_t used;
static volatile bool recording = true;
static volatile uint32_t init_calls;

static void ring_init(void)
{
	init_calls++;
}

static void ring_output(const struct tracing_backend *backend, uint8_t *data, uint32_t length)
{
	ARG_UNUSED(backend);
	unsigned int key = irq_lock();

	if (recording && used + length <= sizeof(ring)) {
		memcpy(&ring[used], data, length);
		used += length;
	}
	irq_unlock(key);
}

static const struct tracing_backend_api ring_api = {
	.init = ring_init,
	.output = ring_output,
};

TRACING_BACKEND_DEFINE(nros_test_ring, ring_api);

int main(void)
{
	/* Scheduling traffic first, so the stream holds more than the rename. */
	for (int i = 0; i < 3; i++) {
		k_msleep(1);
	}
	k_thread_name_set(k_current_get(), THREAD_NAME);

	unsigned int key = irq_lock();

	recording = false;
	irq_unlock(key);

	printk("tracing-backend: %s\n", CONFIG_TRACING_BACKEND_CUSTOM_NAME);
	printk("tracing-init-calls: %u\n", init_calls);
	printk("tracing-thread: %08x\n", (uint32_t)(uintptr_t)k_current_get());
	printk("tracing-bytes: %zu\n", used);
	for (size_t off = 0; off < used; off += 32) {
		printk("tracing-hex: ");
		for (size_t i = off; i < used && i < off + 32; i++) {
			printk("%02x", ring[i]);
		}
		printk("\n");
	}
	printk("tracing-done\n");
	return 0;
}
