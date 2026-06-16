FROM rust:1.80-slim-bullseye AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
WORKDIR /app
COPY --from=builder /app/target/release/cdd-storage /usr/local/bin/cdd-storage
EXPOSE 8080
ENTRYPOINT ["cdd-storage"]
