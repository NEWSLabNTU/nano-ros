# Phase 173 — unified-jobserver build orchestration.
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
FIXTURES  := $(addprefix fixtures-,$(PLATFORMS))

.PHONY: all prereqs build-examples $(FIXTURES)

# build-examples + every platform's fixtures run concurrently; all gated
# behind the shared prereqs (workspace + bindings + zenoh posix fixture).
all: build-examples $(FIXTURES)

# Serial prerequisites every parallel target needs. Each `+just` shares
# the jobserver, so the cargo/cc inside still parallelizes against the
# pool — only the three steps themselves are ordered.
prereqs:
	+just build-workspace
	+just generate-bindings
	+just build-zenoh-posix-fixture

build-examples: prereqs
	+just build-examples

$(FIXTURES): fixtures-%: prereqs
	+just $* build-fixtures
