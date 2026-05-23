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

PLATFORMS := native qemu freertos nuttx threadx_linux threadx_riscv64 zephyr stm32f4
EXAMPLE_OVERLAP_PLATFORMS := native freertos threadx_linux threadx_riscv64
INDEPENDENT_FIXTURE_PLATFORMS := qemu nuttx zephyr stm32f4
OVERLAP_FIXTURES := $(addprefix fixtures-,$(EXAMPLE_OVERLAP_PLATFORMS))
INDEPENDENT_FIXTURES := $(addprefix fixtures-,$(INDEPENDENT_FIXTURE_PLATFORMS))
FIXTURES := $(OVERLAP_FIXTURES) $(INDEPENDENT_FIXTURES)

.PHONY: all prereqs build-example-extras $(FIXTURES)

# Build non-fixture example leaves + every platform's fixtures concurrently;
# all gated behind shared workspace/tooling prereqs.
all: build-example-extras $(FIXTURES)

# Serial prerequisites every parallel target needs. Each `+just` shares
# the jobserver, so the cargo/cc inside still parallelizes against the pool.
prereqs:
	+just generate-bindings
	+just build-workspace
	+just build-workspace-embedded
	+just build-zenohd
	+just qemu build-zenoh-pico
	+just build-zenoh-posix-fixture

build-example-extras: prereqs
	+just build-example-extras

$(INDEPENDENT_FIXTURES): fixtures-%: prereqs
	+just $* build-fixtures

$(OVERLAP_FIXTURES): fixtures-%: prereqs
	+just $* build-fixtures
