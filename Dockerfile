# official plex image is old, bullseye required
FROM rust:1.91.1-alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    build-base

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release


FROM alpine

COPY --from=builder /usr/src/app/target/release/plex-proxy /usr/local/bin/plex-proxy

ENTRYPOINT ["/usr/local/bin/plex-proxy"]
