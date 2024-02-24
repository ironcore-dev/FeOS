FROM rust:1-bookworm

RUN apt-get update && apt-get upgrade -y && apt-get install -y \
    flex bison bc libelf-dev \
    cpio zstd \
    gperf \
    ninja-build \
    protobuf-compiler \
    libprotobuf-dev \
    sbsigntool python3-pefile systemd-boot \
    musl-tools

RUN cargo new xyz; cd xyz; cargo fetch; cd ..; rm -rf xyz
RUN rustup component add clippy
RUN rustup target add x86_64-unknown-linux-musl
RUN mkdir /feos
RUN mkdir /target
RUN wget https://raw.githubusercontent.com/systemd/systemd/main/src/ukify/ukify.py -O /usr/bin/ukify && chmod +x /usr/bin/ukify
WORKDIR /feos
