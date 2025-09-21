use axum::{extract::{Path, Query, State}, http::{StatusCode, header, HeaderMap}, response::{IntoResponse, Response}, body::Body};
use serde::Deserialize;
use crate::{AppState};
use crate::s3::{models::*, xml};
use crate::storage::posix;
use fs_err as fs;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use md5::Context as Md5Context;
use tokio::fs as tfs;

pub async fn service_root() -> impl IntoResponse {
    (StatusCode::OK, "")
}

pub async fn cors_preflight() -> impl IntoResponse {
    let mut resp = Response::builder().status(StatusCode::NO_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, PUT, POST, DELETE, HEAD, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .body(Body::empty()).unwrap();
    resp
}

pub async fn bucket_list(State(state): State<AppState>) -> impl IntoResponse {
    // Enumerate buckets under data_root
    let mut buckets = Vec::new();
    if let Ok(rd) = fs::read_dir(&state.cfg.data_root) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                buckets.push(Bucket { Name: e.file_name().to_string_lossy().into_owned(), CreationDate: chrono::Utc::now().to_rfc3339() })
            }
        }
    }
    let result = ListBucketsResult { Owner: Owner { ID: "gateway".into(), DisplayName: "gateway".into() }, Buckets: Buckets { Bucket: buckets } };
    let xml_body = xml::to_xml(&result, "ListAllMyBucketsResult");
    ([(header::CONTENT_TYPE, "application/xml")], xml_body)
}

pub async fn head_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> impl IntoResponse {
    let dir = posix::bucket_dir(&state.cfg, &bucket);
    if dir.is_dir() { StatusCode::OK } else { StatusCode::NOT_FOUND }
}

pub async fn create_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> Response {
    let dir = posix::bucket_dir(&state.cfg, &bucket);
    if dir.exists() { return Response::builder().status(StatusCode::CONFLICT).body(Body::from("BucketAlreadyOwnedByYou")).unwrap(); }
    if let Err(e) = tfs::create_dir_all(&dir).await { return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap(); }
    Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap()
}

pub async fn delete_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> impl IntoResponse {
    let dir = posix::bucket_dir(&state.cfg, &bucket);
    match tfs::remove_dir(&dir).await { // only empty bucket
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::CONFLICT,
    }
}

#[derive(Debug, Deserialize)]
pub struct ListV2Query {
    #[serde(rename = "list-type")] pub list_type: Option<u8>,
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "start-after")] pub start_after: Option<String>,
    #[serde(rename = "continuation-token")] pub continuation_token: Option<String>,
    #[serde(rename = "max-keys")] pub max_keys: Option<i32>,
    pub location: Option<String>,
}

pub async fn bucket_post(State(state): State<AppState>, Path(_bucket): Path<String>, Query(_q): Query<ListV2Query>) -> impl IntoResponse {
    // Placeholder for operations like ListObjectsV2 via POST (for AWS SDKs)
    (StatusCode::NOT_IMPLEMENTED, "NotImplemented")
}

pub async fn list_objects_v2(State(state): State<AppState>, Path(bucket): Path<String>, Query(q): Query<ListV2Query>) -> impl IntoResponse {
    // Handle GetBucketLocation
    if q.location.is_some() {
        let body = format!("<LocationConstraint xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">{}</LocationConstraint>", state.cfg.region);
        return ([(header::CONTENT_TYPE, "application/xml")], body);
    }
    let prefix = q.prefix.unwrap_or_default();
    let max_keys = q.max_keys.unwrap_or(1000).min(1000);
    let base = posix::bucket_dir(&state.cfg, &bucket);
    let mut contents = Vec::new();
    let mut count = 0;
    let delimiter = q.delimiter;
    let mut common_prefixes: Vec<CommonPrefix> = Vec::new();

    let start_after = q.start_after.unwrap_or_default();
    let continuation = q.continuation_token.unwrap_or_default();
    let start_marker = if !continuation.is_empty() { continuation } else { start_after };
    if base.is_dir() {
        let mut keys: Vec<(String, u64)> = Vec::new();
        for entry in walkdir::WalkDir::new(&base).min_depth(0).max_depth(usize::MAX).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                let p = entry.path().to_path_buf();
                if p.extension().map(|e| e == "json").unwrap_or(false) && p.file_name().and_then(|f| f.to_str()).map(|n| n.ends_with(".meta.json")).unwrap_or(false) { continue; }
                let rel = p.strip_prefix(&base).unwrap().to_string_lossy().to_string();
                if !rel.starts_with(&prefix) { continue; }
                if !start_marker.is_empty() && rel <= start_marker { continue; }
                if let Ok(md) = fs::metadata(&p) { keys.push((rel, md.len())); }
            }
        }
        keys.sort();
        for (rel, size) in keys.into_iter() {
            if let Some(ref d) = delimiter {
                if let Some(idx) = rel[prefix.len()..].find(d) {
                    let cp = rel[..prefix.len()+idx+1].to_string();
                    if !common_prefixes.iter().any(|c| c.Prefix == cp) { common_prefixes.push(CommonPrefix { Prefix: cp }); }
                    continue;
                }
            }
            contents.push(Object { Key: rel, LastModified: chrono::Utc::now().to_rfc3339(), ETag: String::new(), Size: size, StorageClass: "STANDARD".into() });
            count += 1;
            if count >= max_keys { break; }
        }
    }
    let is_truncated = count >= max_keys;
    let next_cont = if is_truncated { contents.last().map(|o| o.Key.clone()) } else { None };
    let out = ListObjectsV2Result { Name: bucket, Prefix: Some(prefix), Delimiter: delimiter.clone(), KeyCount: contents.len() as i32, MaxKeys: max_keys, IsTruncated: is_truncated, Contents: contents, CommonPrefixes: if common_prefixes.is_empty() { None } else { Some(common_prefixes) }, NextContinuationToken: next_cont };
    let body = xml::to_xml(&out, "ListBucketResult");
    ([(header::CONTENT_TYPE, "application/xml")], body)
}

pub async fn head_object(State(state): State<AppState>, Path((bucket, key)): Path<(String, String)>) -> impl IntoResponse {
    let (data, meta) = posix::object_paths(&state.cfg, &bucket, &key);
    if data.is_file() {
        if let Ok(m) = fs::metadata(&data) { return (StatusCode::OK, [(header::CONTENT_LENGTH, m.len().to_string())]).into_response(); }
        return StatusCode::OK.into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

pub async fn delete_object(State(state): State<AppState>, Path((bucket, key)): Path<(String, String)>) -> impl IntoResponse {
    let (data, meta) = posix::object_paths(&state.cfg, &bucket, &key);
    let _ = tfs::remove_file(&data).await;
    let _ = tfs::remove_file(&meta).await;
    StatusCode::NO_CONTENT
}

pub async fn put_object(State(state): State<AppState>, Path((bucket, key)): Path<(String, String)>, headers: HeaderMap, body: Body) -> Response {
    use futures::StreamExt;
    // Handle CopyObject
    if let Some(src) = headers.get("x-amz-copy-source").and_then(|v| v.to_str().ok()) {
        let src = src.trim_start_matches('/');
        let (src_bucket, src_key) = match src.split_once('/') { Some((b,k)) => (b.to_string(), k.to_string()), None => (bucket.clone(), src.to_string()) };
        let (src_data, src_meta) = posix::object_paths(&state.cfg, &src_bucket, &src_key);
        let (dst_data, dst_meta) = posix::object_paths(&state.cfg, &bucket, &key);
        if !src_data.is_file() { return Response::builder().status(StatusCode::NOT_FOUND).body(Body::from("NoSuchKey")).unwrap(); }
        if let Err(e) = posix::ensure_parent_dirs(&dst_data).await { return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap(); }
        if let Err(e) = tfs::copy(&src_data, &dst_data).await { return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap(); }
        if let Ok(bytes) = posix::read_file(&src_meta).await { let _ = posix::write_file_atomic(&dst_meta, &bytes).await; }
        // Compute ETag of dest
        let data = match posix::read_file(&dst_data).await { Ok(d) => d, Err(e) => return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap() };
        let mut hasher = Md5Context::new(); hasher.consume(&data); let etag = format!("\"{:x}\"", hasher.compute());
        let xml_body = format!("<CopyObjectResult><LastModified>{}</LastModified><ETag>{}</ETag></CopyObjectResult>", chrono::Utc::now().to_rfc3339(), etag);
        return Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE, "application/xml").body(Body::from(xml_body)).unwrap();
    }
    let (data, meta) = posix::object_paths(&state.cfg, &bucket, &key);
    if let Err(e) = posix::ensure_parent_dirs(&data).await { return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap(); }
    let mut f = match tfs::File::create(&data).await { Ok(f) => f, Err(e) => return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap() };
    let mut stream = body.into_data_stream();
    let mut hasher = Md5Context::new();
    while let Some(chunk) = stream.next().await { match chunk { Ok(bytes) => { hasher.consume(&bytes); if let Err(e) = f.write_all(&bytes).await { return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(e.to_string())).unwrap(); } }, Err(e) => return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(e.to_string())).unwrap() } }
    let _ = f.flush().await;
    let etag = format!("\"{:x}\"", hasher.compute());
    let meta_body = serde_json::json!({"etag": etag, "content_type": "application/octet-stream"});
    let _ = posix::write_file_atomic(&meta, meta_body.to_string().as_bytes()).await;
    Response::builder().status(StatusCode::OK).header(header::ETAG, etag).body(Body::empty()).unwrap()
}

pub async fn get_object(State(state): State<AppState>, Path((bucket, key)): Path<(String, String)>, headers: HeaderMap) -> impl IntoResponse {
    let (data, meta) = posix::object_paths(&state.cfg, &bucket, &key);
    if !data.is_file() { return StatusCode::NOT_FOUND.into_response(); }
    let mut file = tfs::File::open(&data).await.unwrap();
    let metadata = file.metadata().await.ok();
    use tokio::io::{AsyncSeekExt, AsyncReadExt};
    let mut status = StatusCode::OK;
    let mut len_header: Option<u64> = None;
    let mut stream_body: Body;
    if let Some(range_val) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some(r) = range_val.strip_prefix("bytes=") {
            if let Some((start_s, end_s)) = r.split_once('-') {
                if let Ok(mut start) = start_s.parse::<u64>() {
                    let total = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let end = if end_s.is_empty() { total - 1 } else { end_s.parse::<u64>().unwrap_or(total - 1) };
                    if start <= end && end < total {
                        status = StatusCode::PARTIAL_CONTENT;
                        let len = end - start + 1;
                        len_header = Some(len);
                        let _ = file.seek(std::io::SeekFrom::Start(start)).await;
                        let limited = file.take(len);
                        let stream = tokio_util::io::ReaderStream::new(limited);
                        stream_body = Body::from_stream(stream);
                        let resp = Response::builder()
                            .status(status)
                            .header(header::ACCEPT_RANGES, "bytes")
                            .header(header::CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, total))
                            .header(header::CONTENT_LENGTH, len)
                            .header(header::CONTENT_TYPE, "application/octet-stream")
                            .body(stream_body)
                            .unwrap();
                        return resp;
                    }
                }
            }
        }
    }
    let stream = tokio_util::io::ReaderStream::new(file);
    stream_body = Body::from_stream(stream);
    let mut headers = vec![(header::CONTENT_TYPE, "application/octet-stream".to_string())];
    if let Ok(bytes) = posix::read_file(&meta).await { if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) { if let Some(etag) = v.get("etag").and_then(|s| s.as_str()) { headers.push((header::ETAG, etag.to_string())); } } }
    let mut resp = Response::builder().status(status).header(header::CONTENT_TYPE, headers.iter().find(|(h,_)| *h==header::CONTENT_TYPE).unwrap().1.as_str());
    if let Some(md) = metadata { resp = resp.header(header::CONTENT_LENGTH, len_header.unwrap_or(md.len())); }
    let resp = resp.body(stream_body).unwrap();
    resp
}

pub async fn object_post(State(_state): State<AppState>, Path((_bucket, _key)): Path<(String, String)>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "NotImplemented")
}


