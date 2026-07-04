ARG RUST_IMAGE=docker.io/library/rust:1-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian13:nonroot

FROM ${RUST_IMAGE} AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --locked --profile deploy

FROM ${RUNTIME_IMAGE}
COPY --from=builder /src/target/deploy/k8sbackup-rs /usr/local/bin/k8sbackup-rs

ENTRYPOINT ["/usr/local/bin/k8sbackup-rs"]
