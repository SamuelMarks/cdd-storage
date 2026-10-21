FROM rust:alpine AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev gcc make pkgconfig
COPY . .
RUN cargo build --release

FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/target/release/cdd-storage /usr/local/bin/cdd-storage
ENV HOST=0.0.0.0
ENV PORT=8085
EXPOSE 8085
ENTRYPOINT ["cdd-storage"]
