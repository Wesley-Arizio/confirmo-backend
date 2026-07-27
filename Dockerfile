# Single parametrized build for every service in the workspace.
# docker-compose passes PACKAGE (cargo package name) and BIN (binary name).
#   confirmo-auth                 -> PACKAGE=confirmo-auth                 BIN=confirmo-auth
#   confirmo-graphql (pkg name is "confirmo") -> PACKAGE=confirmo          BIN=confirmo
#   confirmo-notification-service -> PACKAGE=confirmo-notification-service BIN=confirmo-notification-service

FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake \
        protobuf-compiler \
        libprotobuf-dev \
        pkg-config \
        libssl-dev \
        libsasl2-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

ARG PACKAGE
ARG BIN

# Compile the selected package and copy its binary out. The cargo registry and target
# are cached across builds (sharing=locked so the parallel compose builds don't corrupt the shared cache). 
# The binary is copied out here because cache mounts don't persist.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release -p "${PACKAGE}" \
    && cp "/app/target/release/${BIN}" /app/service

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        libsasl2-2 \
        zlib1g \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/service /usr/local/bin/service

ENTRYPOINT ["/usr/local/bin/service"]
