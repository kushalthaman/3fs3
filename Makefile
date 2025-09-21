SHELL := /bin/bash
BIN := threefs-s3-gateway
CRATE := crates/gateway
IMAGE ?= 3fs3

.PHONY: all build run test fmt clippy docker docker-push helm-lint

all: build

build:
	cargo build --workspace --locked

release:
	cargo build --workspace --release --locked

run:
	cargo run -p threefs-s3-gateway

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-features -- -D warnings

test:
	cargo test --workspace --all-features -- --nocapture

docker:
	docker build -t $(IMAGE):dev .

helm-lint:
	helm lint deploy/helm

