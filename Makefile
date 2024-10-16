clippy:
	cargo clippy

all: clippy
	cargo build --target=x86_64-unknown-linux-musl --all

release: clippy
	cargo build --release --target=x86_64-unknown-linux-musl --all

run: all
	./target/debug/feos

clean:
	rm -rf target

test: clippy
	cargo test

test-ci:
	cargo test

include hack/hack.mk

feos_client:
	cd client && cargo build
