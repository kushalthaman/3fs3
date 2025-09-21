# 3FS S3 Gateway

S3-compatible gateway backed by 3FS that makes sure of a single FUSE mount per node and exposes an S3 HTTP API supporting a feature set for standard S3 clients.

## features

- Buckets: Create/Delete/Head/List, GetBucketLocation (static region)
- Objects: Put/Get (Range planned), Head, Delete, CopyObject (planned), Put/Get Object Tagging (planned), basic CORS (planned)
- Multipart: Create/UploadPart/Complete/Abort/ListParts (planned)
- ETags: MD5 for single-part; S3-style composed ETag for multipart (planned)
- Presigned URLs: GET/PUT

## data layout on 3FS

- Global mount: `/var/lib/3fs/mnt/<cluster_id>`
- Buckets: `${MOUNT}/buckets/<bucket>/`
- Objects: `${MOUNT}/buckets/<bucket>/<key>`
- Object metadata: `${objectPath}.meta.json`
- Multipart temp: `${MOUNT}/.multipart/<bucket>/<uploadId>/<partNumber>`

## quickstart for local dev

```bash
make build
export CLUSTER_ID=local
export MOUNTPOINT=${MOUNTPOINT:-/tmp/3fs/local}
export DATA_ROOT=${DATA_ROOT:-$MOUNTPOINT/buckets}
export ACCESS_KEY=test
export SECRET_KEY=testsecret
mkdir -p "$DATA_ROOT" "$MOUNTPOINT/.multipart"
./scripts/dev-run.sh
# In another shell
./deploy/examples/awscli.sh
```

## kubernetes (Helm)

```bash
helm upgrade --install threefs-s3 deploy/helm -n storage \
  --create-namespace \
  --set clusterId=stage \
  --set mgmtdAddresses="RDMA://192.168.1.1:8000" \
  --set s3.accessKey=YOURKEY --set s3.secretKey=YOURSECRET
```

## 3FS reqs

- FUSE binary: `/opt/3fs/bin/hf3fs_fuse_main`
- Token file: e.g. `/opt/3fs/etc/token.txt`
- mgmtd addresses: `RDMA://host:port`

this renders `hf3fs_fuse_main_launcher.toml` and makes sure of a single global mount per node.

## general notes

- Format: `make fmt`
- Lint: `make clippy`
- Test: `make test`