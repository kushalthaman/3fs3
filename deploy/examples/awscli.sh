#!/usr/bin/env bash
set -euo pipefail

EP=${EP:-http://127.0.0.1:9000}
export AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID:-test}
export AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY:-testsecret}
export AWS_EC2_METADATA_DISABLED=true

aws --endpoint-url "$EP" s3 mb s3://bkt || true
echo hello > /tmp/hello.txt
aws --endpoint-url "$EP" s3 cp /tmp/hello.txt s3://bkt/hello.txt
aws --endpoint-url "$EP" s3 ls s3://bkt
aws --endpoint-url "$EP" s3 cp s3://bkt/hello.txt -

