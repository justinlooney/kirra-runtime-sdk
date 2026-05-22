# ── Stage 1: build ───────────────────────────────────────────────────────────
FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev gcc

WORKDIR /build
COPY . .

RUN cargo build --release --bin aegis_verifier_service

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM alpine:3

RUN apk add --no-cache curl && \
    addgroup -S -g 1000 aegis && \
    adduser  -S -u 1000 -G aegis -h /var/lib/aegis -s /sbin/nologin aegis && \
    mkdir -p /var/lib/aegis && \
    chown aegis:aegis /var/lib/aegis

COPY --from=builder /build/target/release/aegis_verifier_service /usr/local/bin/aegis_verifier_service

USER aegis
WORKDIR /var/lib/aegis

ENV AEGIS_VERIFIER_ADDR=0.0.0.0:8090
ENV AEGIS_DB_PATH=/var/lib/aegis/aegis.db

VOLUME ["/var/lib/aegis"]
EXPOSE 8090

HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -fsSL http://localhost:8090/health || exit 1

ENTRYPOINT ["/usr/local/bin/aegis_verifier_service"]
