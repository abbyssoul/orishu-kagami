# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.94

FROM rust:${RUST_VERSION}-trixie AS builder
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release \
      -p orishu-worker \
      -p orishuctl \
      -p orishu-monitor \
    && mkdir -p /out \
    && cp target/release/orishu-worker /out/ \
    && cp target/release/orishuctl /out/ \
    && cp target/release/orishu-monitor /out/

FROM debian:trixie-slim AS runtime-files
RUN mkdir -p /var/lib/orishu /run/orishu /etc/orishu/tls /work \
    && chown -R 65532:65532 /var/lib/orishu /run/orishu /etc/orishu /work

FROM gcr.io/distroless/cc-debian13:nonroot AS orishu-worker
ARG CREATED
ARG VERSION=0.0.0-dev
ARG REVISION=unknown
LABEL org.opencontainers.image.title="orishu-worker" \
      org.opencontainers.image.description="Distributed simulation worker for Orishu Kagami" \
      org.opencontainers.image.source="https://github.com/abbyssoul/orishu-kagami" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}" \
      org.opencontainers.image.licenses="Apache-2.0"
WORKDIR /var/lib/orishu
COPY --from=runtime-files /var/lib/orishu /var/lib/orishu
COPY --from=runtime-files /run/orishu /run/orishu
COPY --from=runtime-files /etc/orishu /etc/orishu
COPY --from=builder /out/orishu-worker /usr/local/bin/orishu-worker
EXPOSE 6680
ENTRYPOINT ["/usr/local/bin/orishu-worker"]
CMD ["--listen.clients=0.0.0.0:6680"]

FROM gcr.io/distroless/cc-debian13:nonroot AS orishu-ctl
ARG CREATED
ARG VERSION=0.0.0-dev
ARG REVISION=unknown
LABEL org.opencontainers.image.title="orishuctl" \
      org.opencontainers.image.description="Operator CLI for Orishu Kagami" \
      org.opencontainers.image.source="https://github.com/abbyssoul/orishu-kagami" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}" \
      org.opencontainers.image.licenses="Apache-2.0"
WORKDIR /work
COPY --from=runtime-files /work /work
COPY --from=builder /out/orishuctl /usr/local/bin/orishuctl
ENTRYPOINT ["/usr/local/bin/orishuctl"]
CMD ["--help"]

FROM debian:trixie-slim AS orishu-monitor
ARG CREATED
ARG VERSION=0.0.0-dev
ARG REVISION=unknown
LABEL org.opencontainers.image.title="orishu-monitor" \
      org.opencontainers.image.description="Experimental terminal monitor for Orishu Kagami" \
      org.opencontainers.image.source="https://github.com/abbyssoul/orishu-kagami" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}" \
      org.opencontainers.image.licenses="Apache-2.0"
RUN useradd --create-home --uid 10001 --shell /usr/sbin/nologin orishu
WORKDIR /home/orishu
COPY --from=builder /out/orishu-monitor /usr/local/bin/orishu-monitor
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/orishu-monitor"]
