FROM rust:1.80 as builder

WORKDIR /usr/src/app

# Copy workspace
COPY . .

# Build the blackteaspeak_server binary
WORKDIR /usr/src/app/BlackTeaSpeak-Server
RUN cargo build --release --bin blackteaspeak_server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Copy the built binary
COPY --from=builder /usr/src/app/BlackTeaSpeak-Server/target/release/blackteaspeak_server /usr/local/bin/

# Expose ports
# UDP Voice / Desktop Handshake
EXPOSE 9987/udp
# TCP Web Compatibility
EXPOSE 9987/tcp
# ServerQuery
EXPOSE 10101/tcp
# File Transfer
EXPOSE 30303/tcp
# Web Client
EXPOSE 80/tcp
EXPOSE 443/tcp

# Run the binary with serve-all
ENTRYPOINT ["blackteaspeak_server", "serve-all", "--desktop-bind", "0.0.0.0:9987", "--web-bind", "0.0.0.0:9987", "--query-bind", "0.0.0.0:10101", "--file-bind", "0.0.0.0:30303", "--web-client-bind", "0.0.0.0:443"]
