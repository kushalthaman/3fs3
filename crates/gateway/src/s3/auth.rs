use axum::{http::{Request, StatusCode, HeaderMap, HeaderValue, Uri, Method}, response::Response};
use tower::{Layer, Service};
use std::task::{Context, Poll};
use std::pin::Pin;
use crate::config::GatewayConfig;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use http::header::{AUTHORIZATION, HOST};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::collections::BTreeMap;
use anyhow::Context as _;

#[derive(Clone)]
pub struct SigV4Layer { cfg: GatewayConfig }

impl SigV4Layer { pub fn new(cfg: GatewayConfig) -> Self { Self { cfg } } }

impl<S> Layer<S> for SigV4Layer {
    type Service = SigV4Middleware<S>;
    fn layer(&self, inner: S) -> Self::Service { SigV4Middleware { inner, cfg: self.cfg.clone() } }
}

#[derive(Clone)]
pub struct SigV4Middleware<S> { inner: S, cfg: GatewayConfig }

impl<S, B> Service<Request<B>> for SigV4Middleware<S>
where S: Service<Request<B>, Response = Response> + Clone + Send + 'static + Unpin,
      S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> { Pin::new(&mut self.inner).poll_ready(cx) }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        // For MVP: allow unauthenticated health/metrics
        let path = req.uri().path();
        if path == "/healthz" || path == "/readyz" || path == "/metrics" { return self.inner.call(req); }
        if self.cfg.auth_disabled { return self.inner.call(req); }

        match verify_sigv4(&self.cfg, req.method(), req.uri(), req.headers()) {
            Ok(()) => self.inner.call(req),
            Err(_e) => {
                // For security, avoid leaking details
                let mut r = Response::new(axum::body::Body::from("SignatureDoesNotMatch"));
                *r.status_mut() = StatusCode::FORBIDDEN;
                self.inner.call(req)
            }
        }
    }
}

// RFC 3986 unreserved: ALPHA / DIGIT / '-' / '.' / '_' / '~'
const AWS_QUERY_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-').remove(b'_').remove(b'.').remove(b'~');

fn hex_sha256(input: &str) -> String { hex::encode(Sha256::digest(input.as_bytes())) }

fn canonical_uri(uri: &Uri) -> String {
    // Use the path as-is; avoid normalizing for MVP
    let p = uri.path();
    if p.is_empty() { "/".to_string() } else { p.to_string() }
}

fn canonical_query(uri: &Uri) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(q) = uri.query() {
        for part in q.split('&') {
            if part.is_empty() { continue; }
            let mut iter = part.splitn(2, '=');
            let k = iter.next().unwrap_or("");
            let v = iter.next().unwrap_or("");
            pairs.push((k.to_string(), v.to_string()));
        }
        pairs.sort_by(|a,b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    }
    pairs.into_iter().map(|(k,v)| {
        format!("{}={}", utf8_percent_encode(&k, AWS_QUERY_ENCODE), utf8_percent_encode(&v, AWS_QUERY_ENCODE))
    }).collect::<Vec<_>>().join("&")
}

fn signed_headers_list(hmap: &HeaderMap, signed: &str) -> (String, String) {
    // returns (canonical-headers, signed-headers)
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for name in signed.split(';') {
        let lname = name.trim().to_ascii_lowercase();
        if let Some(val) = hmap.get(&lname) {
            let val_str = collapse_ws(val);
            headers.insert(lname, val_str);
        }
    }
    let mut canonical = String::new();
    let mut list = Vec::new();
    for (k,v) in headers.iter() {
        canonical.push_str(&format!("{}:{}\n", k, v));
        list.push(k.clone());
    }
    (canonical, list.join(";"))
}

fn collapse_ws(val: &HeaderValue) -> String {
    let s = val.to_str().unwrap_or("");
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_space { out.push(' '); }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn parse_authz(authz: &str) -> Option<(String/*access_key*/, String/*signed_headers*/, String/*signature*/, String/*date*/, String/*scope*/)> {
    // Example: AWS4-HMAC-SHA256 Credential=AKID/20250101/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-date, Signature=...
    if !authz.starts_with("AWS4-HMAC-SHA256 ") { return None; }
    let rest = &authz[19..];
    let mut access_key = String::new();
    let mut signed_headers = String::new();
    let mut signature = String::new();
    let mut scope = String::new();
    let mut amz_date = String::new();
    for part in rest.split(',') {
        let p = part.trim();
        if let Some(v) = p.strip_prefix("Credential=") {
            let mut it = v.split('/');
            access_key = it.next().unwrap_or("").to_string();
            let date = it.next().unwrap_or("").to_string();
            scope = it.collect::<Vec<_>>().join("/");
            amz_date = date;
        } else if let Some(v) = p.strip_prefix("SignedHeaders=") {
            signed_headers = v.to_string();
        } else if let Some(v) = p.strip_prefix("Signature=") {
            signature = v.to_string();
        }
    }
    if access_key.is_empty() || signed_headers.is_empty() || signature.is_empty() || scope.is_empty() { return None; }
    Some((access_key, signed_headers, signature, amz_date, scope))
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).unwrap();
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date);
    let k_region = hmac_sha256(&k_date, region);
    let k_service = hmac_sha256(&k_region, service);
    hmac_sha256(&k_service, "aws4_request")
}

fn verify_sigv4(cfg: &GatewayConfig, method: &Method, uri: &Uri, headers: &HeaderMap) -> anyhow::Result<()> {
    // 1) Determine signature source: header or presigned query
    let query = uri.query().unwrap_or("");
    let (access_key, signed_headers, signature, date, scope, is_query) = if query.contains("X-Amz-Signature=") {
        // presigned URL
        let qp: BTreeMap<_,_> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let ak = qp.get("X-Amz-Credential").cloned().unwrap_or_default();
        let mut it = ak.split('/');
        let access_key = it.next().unwrap_or("").to_string();
        let date = qp.get("X-Amz-Date").cloned().unwrap_or_default();
        let scope_rest = ak.splitn(2, '/').nth(1).unwrap_or("").to_string();
        let signature = qp.get("X-Amz-Signature").cloned().unwrap_or_default();
        let signed_headers = qp.get("X-Amz-SignedHeaders").cloned().unwrap_or_else(|| "host".to_string());
        (access_key, signed_headers, signature, date, scope_rest, true)
    } else {
        let authz = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).ok_or_else(|| anyhow::anyhow!("MissingAuthorization"))?;
        let (ak, sh, sig, date, scope) = parse_authz(authz).ok_or_else(|| anyhow::anyhow!("InvalidAuthorization"))?;
        (ak, sh, sig, date, scope, false)
    };

    if access_key != cfg.access_key { return Err(anyhow::anyhow!("InvalidAccessKeyId")); }

    // Ensure host header is present
    if headers.get(HOST).is_none() { return Err(anyhow::anyhow!("MissingHost")); }

    // Payload hash
    let payload_hash = headers.get("x-amz-content-sha256").and_then(|v| v.to_str().ok()).unwrap_or("UNSIGNED-PAYLOAD");

    // Build canonical request
    let can_uri = canonical_uri(uri);
    let can_qs = if is_query { canonical_query(uri) } else { canonical_query(uri) };
    let (can_headers, signed_list) = signed_headers_list(headers, &signed_headers);
    let canonical_request = format!("{}\n{}\n{}\n{}\n{}\n{}", method.as_str(), can_uri, can_qs, can_headers, signed_list, payload_hash);
    let cr_hash = hex_sha256(&canonical_request);

    // Build string to sign
    let amz_date = headers.get("x-amz-date").and_then(|v| v.to_str().ok()).unwrap_or(&date);
    // Scope: <date>/<region>/s3/aws4_request
    let mut scope_parts = scope.split('/');
    let _date_sc = scope_parts.next().unwrap_or("");
    let region_sc = scope_parts.next().unwrap_or(&cfg.region);
    let service_sc = scope_parts.next().unwrap_or("s3");
    let sts = format!("AWS4-HMAC-SHA256\n{}\n{}/{}/{}/aws4_request\n{}", amz_date, date, region_sc, service_sc, cr_hash);

    // Derive signing key and compute signature
    let k = derive_signing_key(&cfg.secret_key, &date, region_sc, service_sc);
    let sig_bytes = hmac_sha256(&k, &sts);
    let calc_sig = hex::encode(sig_bytes);
    if calc_sig != signature { return Err(anyhow::anyhow!("SignatureDoesNotMatch")); }
    Ok(())
}


