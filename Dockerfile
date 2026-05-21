FROM rust:1.78-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin aegis_verifier_service

FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/aegis_verifier_service /usr/local/bin/aegis_verifier_service

ENV AEGIS_VERIFIER_MODE=active
ENV AEGIS_DB_PATH=/data/aegis_verifier.sqlite
ENV RUST_LOG=info

EXPOSE 8088
VOLUME ["/data"]
CMD ["aegis_verifier_service"]
