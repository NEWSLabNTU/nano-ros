set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
# Build Docker image for QEMU ARM development
build:
    @echo "Building Docker image..."
    docker build -t nano-ros-qemu -f tests/qemu-baremetal/Dockerfile .
    @echo "Docker image 'nano-ros-qemu' built successfully!"

# Start Docker container with interactive shell
shell:
    @echo "Starting Docker container..."
    docker run -it --rm \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
        nano-ros-qemu

# Start Docker container with TAP networking support
shell-network:
    @echo "Starting Docker container with networking support..."
    docker run -it --rm \
        --cap-add=NET_ADMIN \
        --device=/dev/net/tun \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
        nano-ros-qemu

# Run bare-metal QEMU talker/listener test using Docker Compose
test-qemu: build
    @echo "Running bare-metal QEMU talker/listener test..."
    @echo "This starts 3 containers: zenohd, talker, and listener"
    @echo ""
    HOST_UID=$(id -u) HOST_GID=$(id -g) docker compose -f tests/qemu-baremetal/docker-compose.yml up --build --abort-on-container-exit
    @docker compose -f tests/qemu-baremetal/docker-compose.yml down -v 2>/dev/null || true

# Run QEMU build inside Docker container
build-qemu: build
    @echo "Building QEMU examples inside Docker..."
    docker run --rm \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
        nano-ros-qemu \
        bash -c "cd /work && just qemu build"

# Start Docker Compose services (QEMU + zenohd)
up:
    docker compose -f docker/docker-compose.yml up -d

# Stop Docker Compose services
down:
    docker compose -f docker/docker-compose.yml down

# Execute command in running Docker container
exec CMD="bash":
    docker compose -f docker/docker-compose.yml exec qemu {{CMD}}

# Run tests inside a Docker container with network privileges.
# Root sets up veth pairs, bridges, and binary capabilities.
# Tests run as the host user (HOST_UID/HOST_GID) — no host setup needed.
#
# Examples:
#   just docker test test-threadx-linux   # ThreadX Linux E2E (needs CAP_NET_RAW)
#   just docker test test-freertos        # FreeRTOS QEMU E2E
#   just docker test test                 # All tests
test RECIPE="test":
    HOST_UID=$(id -u) HOST_GID=$(id -g) \
        docker compose -f docker/docker-compose.yml run --rm test \
        just {{RECIPE}}

# Run `just doctor` inside the Docker container to diagnose install
# status in the canonical dev environment. Useful for reproducing CI
# state on the host: if `just doctor` here says everything is OK, the
# Docker image is ready for `just docker test`.
doctor:
    @just docker test doctor

# Show Docker help
help:
    @echo "Docker Development Environment"
    @echo "==============================="
    @echo ""
    @echo "Build Docker image:"
    @echo "  just docker build            # Build nano-ros-qemu image"
    @echo ""
    @echo "Interactive shell:"
    @echo "  just docker shell            # Start container with shell"
    @echo "  just docker shell-network    # Start with TAP networking support"
    @echo ""
    @echo "Run tests in Docker (root network setup, tests as host user):"
    @echo "  just docker test                      # Run all tests"
    @echo "  just docker test test-threadx-linux    # ThreadX Linux (needs TAP bridge)"
    @echo "  just docker test test-freertos         # FreeRTOS QEMU"
    @echo ""
    @echo "Diagnose setup inside Docker:"
    @echo "  just docker doctor           # Run 'just doctor' in the container"
    @echo ""
    @echo "Run commands in Docker:"
    @echo "  just docker test-qemu        # Run QEMU talker/listener test"
    @echo "  just docker build-qemu       # Build QEMU examples in Docker"
    @echo ""
    @echo "Docker Compose:"
    @echo "  just docker up               # Start development services"
    @echo "  just docker down             # Stop development services"
    @echo "  just docker exec             # Execute command in container"
    @echo ""
    @echo "Benefits:"
    @echo "  - QEMU 7.2 (Debian bookworm) - fixes TAP networking issues"
    @echo "  - Consistent environment across platforms"
    @echo "  - All ARM toolchain and Rust targets pre-installed"
