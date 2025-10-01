IPAM ?=

.PHONY: all clippy release run clean cli test

clippy:
	cargo clippy

all: clippy
	cargo build --target=x86_64-unknown-linux-musl --all

release: clippy
	cargo build --release --features git-version --target=x86_64-unknown-linux-musl --all

run: all
	sudo ./target/debug/feos --ipam $(IPAM)

clean:
	rm -rf target

test: clippy
	cargo test

include hack/hack.mk

cli:
	cargo build --package feos-cli --release
