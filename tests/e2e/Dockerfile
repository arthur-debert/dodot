# Multi-stage Dockerfile for dodot E2E testing.
#
# Stage 1: Build the dodot binary from source.
# Stage 2: Minimal runtime with BATS for test execution.
#
# Usage:
#   docker build -t dodot-e2e -f tests/e2e/Dockerfile .
#   docker run --rm dodot-e2e                    # run all tests
#   docker run --rm -it dodot-e2e bash            # interactive shell
#   docker run --rm dodot-e2e bats /tests/test_up.bats  # single test file

# ── Stage 1: Builder ────────────────────────────────────────────
FROM rust:1.94.1-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/

RUN cargo build --release -p dodot && \
    strip target/release/dodot

# ── Stage 2: Runtime ────────────────────────────────────────────
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install bats-core + helpers
RUN git clone --depth 1 https://github.com/bats-core/bats-core.git /tmp/bats && \
    /tmp/bats/install.sh /usr/local && \
    rm -rf /tmp/bats

# Copy dodot binary from builder
COPY --from=builder /build/target/release/dodot /usr/local/bin/dodot

# Copy test suite
COPY tests/e2e/bats/ /tests/

ENV DODOT_BIN=/usr/local/bin/dodot
WORKDIR /tests

ENTRYPOINT ["bats"]
CMD ["/tests/"]
