.PHONY: all lint test build install

all: test

lint:
	cargo check
	cargo clippy --tests -- -D warnings

test: lint
	cargo test

build:
	cargo build

install:
	cargo install --path .
