IPAM ?=

##@ Development

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

.PHONY: add-license
add-license: addlicense ## Add license headers to all Rust files.
	find . -name '*.rs' -exec "$(ADDLICENSE)" -f hack/license-header.txt {} +

.PHONY: check-license
check-license: addlicense ## Check license headers in all Rust files.
	find . -name '*.rs' -exec "$(ADDLICENSE)" -check -c 'IronCore authors' {} +

include hack/hack.mk

cli:
	cargo build --package feos-cli --release

##@ Tools

LOCALBIN ?= $(shell pwd)/bin
$(LOCALBIN):
	mkdir -p $(LOCALBIN)

ADDLICENSE ?= $(LOCALBIN)/addlicense
ADDLICENSE_VERSION ?= v1.1.1

.PHONY: addlicense
addlicense: $(ADDLICENSE) ## Download addlicense locally if necessary.
$(ADDLICENSE): $(LOCALBIN)
	GOBIN=$(LOCALBIN) go install github.com/google/addlicense@$(ADDLICENSE_VERSION)
