FROM rust:1.95.0-slim-bookworm AS builder

RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build -p orchy-server -p orchy-cli --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/orchy-server /usr/local/bin/orchy-server
COPY --from=builder /app/target/release/orchy-cli /usr/local/bin/orchy

WORKDIR /app

CMD ["orchy-server"]
