# syntax=docker/dockerfile:1

ARG RUST_IMAGE=docker.io/library/rust:1-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian13:nonroot

FROM ${RUST_IMAGE} AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src

# The registry and target dirs are cache mounts, so their contents don't land
# in the image layer; copy the binary out to a normal path before it unmounts.
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/src/target,id=cargo-target \
    cargo build --locked --profile deploy \
    && cp target/deploy/k8sbackup-rs /tmp/k8sbackup-rs

FROM ${RUNTIME_IMAGE}
COPY --from=builder /tmp/k8sbackup-rs /usr/local/bin/k8sbackup-rs

ENTRYPOINT ["/usr/local/bin/k8sbackup-rs"]
