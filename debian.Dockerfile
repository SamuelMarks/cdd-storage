FROM rust:slim-bullseye AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
WORKDIR /app
COPY --from=builder /app/target/release/cdd-storage /usr/local/bin/cdd-storage
ENV HOST=0.0.0.0
ENV PORT=8085
EXPOSE 8085
ENTRYPOINT ["cdd-storage"]
