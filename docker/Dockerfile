FROM rust:bullseye AS builder

WORKDIR /app

RUN git clone https://github.com/nappa85/cah_bot.git && cd cah_bot && cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y libsqlite3-0 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/cah_bot/target/release/cah_bot /usr/local/bin/cah_bot

RUN chmod +x /usr/local/bin/cah_bot

WORKDIR /usr/local/bin/

CMD cah_bot