# official plex image is old, bullseye required
FROM rust:1.82.0-bullseye AS builder

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release


FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/plex-proxy /usr/local/bin/plex-proxy

ENTRYPOINT ["/usr/local/bin/plex-proxy"]
