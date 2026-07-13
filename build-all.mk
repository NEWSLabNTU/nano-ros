# Phase 176 — unified-jobserver build orchestration.
#
# Run via `just build-all-jobserver` (or directly:
#   third-party/make/make -j$(nproc) --jobserver-style=fifo -f build-all.mk
# ). The pinned make >=4.4 hands out a fifo-based jobserver; every
# `+`-prefixed recipe below shares it, and because fifo auth is a PATH
# (not inherited fds) every descendant tool — cargo, its build-script
# `cc`, ninja-via-west (>=1.13), cmake's generator — joins the SAME
# token pool by opening the fifo named in MAKEFLAGS. So the whole build
# draws from one dynamically-allocated pool of `-j` tokens instead of
# the static per-platform split: when the fast platforms finish, their
# tokens flow to the long pole (zephyr) automatically.
#
# Requirements (checked by `just workspace doctor`):
#   - make >=4.4   (third-party/make/make) — fifo jobserver provider
#   - ninja >=1.13 (third-party/ninja/ninja) — fifo jobserver client
# Both must be first on PATH (direnv .envrc) so the sub-tools pick them
# up. Tools must NOT be passed an explicit -j / --parallel (that detaches
# them from the pool); the `just` recipes default their inner fan-out to
# ${NROS_BUILD_JOBS}, which the wrapper sets high so the jobserver — not
# GNU parallel — is the real throttle.

PLATFORMS := native qemu freertos nuttx threadx_linux threadx_riscv64 zephyr stm32f4 esp32 px4
EXAMPLE_OVERLAP_PLATFORMS := native freertos threadx_linux threadx_riscv64
# esp32: independent fixture tree (n-esp32-qemu + logging-smoke-esp32-qemu);
# `just esp32 build-fixtures` chains build-logging-smoke. Must be here, not
# only in PLATFORMS — test-all exercises the esp32 fixtures, so build-all
# (the top build tier) must build them or the suite hard-fails "not built".
INDEPENDENT_FIXTURE_PLATFORMS := qemu nuttx zephyr stm32f4 esp32 px4
OVERLAP_FIXTURES := $(addprefix fixtures-,$(EXAMPLE_OVERLAP_PLATFORMS))
INDEPENDENT_FIXTURES := $(addprefix fixtures-,$(INDEPENDENT_FIXTURE_PLATFORMS))
FIXTURES := $(OVERLAP_FIXTURES) $(INDEPENDENT_FIXTURES)

BUILD_ALL_LOG_DIR ?= $(NROS_BUILD_LOG_DIR)
ifeq ($(strip $(BUILD_ALL_LOG_DIR)),)
BUILD_ALL_LOG_DIR := tmp/build-all-$(shell date +%Y%m%d-%H%M%S)-$(shell mktemp -u XXXXXXXX)
endif
BUILD_ALL_JOBLOG := $(BUILD_ALL_LOG_DIR)/build-all.joblog

.PHONY: all prereqs build-example-extras $(FIXTURES)

define timed_stage
+@mkdir -p tmp; \
	mkdir -p "$(BUILD_ALL_LOG_DIR)"; \
	start=$$(date +%s); \
	status=0; \
	echo "==> $(1)"; \
	$(2) || status=$$?; \
	end=$$(date +%s); \
	printf '%s\t%s\t%s\t%s\t%s\n' '$(1)' "$$start" "$$end" "$$((end - start))" "$$status" >> "$(BUILD_ALL_JOBLOG)"; \
	exit $$status
endef

# Build non-fixture example leaves + every platform's fixtures concurrently;
# all gated behind shared workspace/tooling prereqs. Drop the same
# `.fixtures-built` stamp the public `build-test-fixtures` writes (justfile)
# so the `_require-fixtures` preflight lets `test-all` run after `build-all`
# — build-all builds every fixture, so it must vouch for them too.
all: build-example-extras $(FIXTURES)
	@mkdir -p target/nextest
	@date -u +%Y-%m-%dT%H:%M:%SZ > target/nextest/.fixtures-built
	@echo "build-all: stamped target/nextest/.fixtures-built"

# Serial prerequisites every parallel target needs. Each `+just` shares
# the jobserver, so the cargo/cc inside still parallelizes against the pool.
prereqs:
	@mkdir -p tmp "$(BUILD_ALL_LOG_DIR)"
	@log_dir="$(BUILD_ALL_LOG_DIR)"; case "$$log_dir" in /*) link="$$log_dir";; *) link="$$(pwd)/$$log_dir";; esac; ln -sfn "$$link" tmp/build-all-latest
	@echo "build-all joblog: $(BUILD_ALL_JOBLOG)"
	@printf 'stage\tstart_epoch\tend_epoch\tduration_seconds\tstatus\n' > "$(BUILD_ALL_JOBLOG)"
	$(call timed_stage,prereq: generate-bindings,just generate-bindings)
	$(call timed_stage,prereq: build-workspace,just build-workspace)
	$(call timed_stage,prereq: build-workspace-embedded,just build-workspace-embedded)
	$(call timed_stage,prereq: build-zenohd,just build-zenohd)
	$(call timed_stage,prereq: qemu build-zenoh-pico,just qemu build-zenoh-pico)
	$(call timed_stage,prereq: build-zenoh-posix-fixture,just build-zenoh-posix-fixture)

build-example-extras: prereqs
	$(call timed_stage,build-example-extras,just build-example-extras)

$(INDEPENDENT_FIXTURES): fixtures-%: prereqs
	$(call timed_stage,fixtures-$*,just $* build-fixtures)

$(OVERLAP_FIXTURES): fixtures-%: prereqs
	$(call timed_stage,fixtures-$*,just $* build-fixtures)
