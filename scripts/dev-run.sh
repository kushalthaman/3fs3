#!/usr/bin/env bash
set -euo pipefail

export RUST_LOG=${RUST_LOG:-info}
export CLUSTER_ID=${CLUSTER_ID:-local}
export MOUNTPOINT=${MOUNTPOINT:-/tmp/3fs/$CLUSTER_ID}
export DATA_ROOT=${DATA_ROOT:-$MOUNTPOINT/buckets}
export BIND_ADDRESS=${BIND_ADDRESS:-127.0.0.1:9000}
export ACCESS_KEY=${ACCESS_KEY:-test}
export SECRET_KEY=${SECRET_KEY:-testsecret}

mkdir -p "$MOUNTPOINT/.multipart" "$DATA_ROOT"

cargo run -p threefs-s3-gateway

