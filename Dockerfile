FROM rust:latest

WORKDIR /mnt/src

RUN apt-get install libc6-dev
RUN rustup target add aarch64-unknown-linux-gnu

CMD RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-gnu --release