use anyhow::{Context, Result};
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    primitives::ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
};
use axum::{
    body::{to_bytes, Body},
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use common::{derive_kid, AppConfig, ServiceJwtClaims, ServiceTlsConfig};
use hmac::{Hmac, Mac};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use proto::{
    file_v1::{
        file_service_client::FileServiceClient, ConfirmFileRequest, CreateFileRequest,
        DeleteFileRequest, ListFilesRequest,
    },
    key_validation_v1::{
        key_validation_service_client::KeyValidationServiceClient, ValidateKeyRequest,
    },
};
use sha2::{Digest, Sha256};
use std::time::Instant;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use tonic::{
    metadata::MetadataValue,
    transport::{Channel, Endpoint},
    Request,
};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;
const DEFAULT_MAX_UPLOAD: usize = 128 * 1024 * 1024;

#[derive(Clone)]
struct App {
    r2: aws_sdk_s3::Client,
    bucket: String,
    key_endpoint: String,
    file_endpoint: String,
    jwt_secret: String,
    jwt_issuer: String,
    jwt_subject: String,
    tls: ServiceTlsConfig,
    max_single_upload: usize,
    max_part_size: usize,
    metrics: PrometheusHandle,
}

#[derive(Clone)]
struct Authorized {
    user_id: Uuid,
}

#[derive(Debug)]
struct S3Error(StatusCode, &'static str);

impl IntoResponse for S3Error {
    fn into_response(self) -> Response {
        let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>{}</Code><Message>{}</Message></Error>", self.1, self.1);
        (self.0, [("content-type", "application/xml")], body).into_response()
    }
}

fn error(status: StatusCode, code: &'static str) -> S3Error {
    S3Error(status, code)
}

impl App {
    async fn grpc_channel(&self, endpoint: &str) -> Result<Channel, S3Error> {
        let ep = Endpoint::from_shared(endpoint.to_owned())
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?;
        let ep = if self.tls.is_enabled() {
            common::tls::configure_client_endpoint(ep, endpoint, &self.tls)
                .await
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?
        } else {
            ep
        };
        ep.connect()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))
    }

    async fn service_token(&self) -> Result<String, S3Error> {
        let claims = ServiceJwtClaims::new(&self.jwt_issuer, &self.jwt_subject, 60)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?;
        claims
            .sign_with_kid(
                &derive_kid(self.jwt_secret.as_bytes()),
                self.jwt_secret.as_bytes(),
            )
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))
    }

    async fn authorize(
        &self,
        headers: &HeaderMap,
        required_scope: &str,
        uri: &http::Uri,
        method: &axum::http::Method,
    ) -> Result<Authorized, S3Error> {
        let parsed = parse_sigv4(headers, uri)
            .ok_or_else(|| error(StatusCode::FORBIDDEN, "SignatureDoesNotMatch"))?;
        let mut client =
            KeyValidationServiceClient::new(self.grpc_channel(&self.key_endpoint).await?);
        let mut request = Request::new(ValidateKeyRequest {
            api_key: parsed.access_key.clone(),
            source_ip: None,
            required_scope: Some(required_scope.to_owned()),
        });
        let token = self.service_token().await?;
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?,
        );
        let result = client
            .validate_key(request)
            .await
            .map_err(|_| error(StatusCode::UNAUTHORIZED, "InvalidAccessKeyId"))?
            .into_inner();
        if !result.is_valid {
            return Err(error(StatusCode::UNAUTHORIZED, "InvalidAccessKeyId"));
        }
        let user_id = Uuid::parse_str(&result.user_id)
            .map_err(|_| error(StatusCode::UNAUTHORIZED, "InvalidAccessKeyId"))?;
        if !verify_sigv4(&parsed, headers, uri, method, &parsed.access_key) {
            return Err(error(StatusCode::FORBIDDEN, "SignatureDoesNotMatch"));
        }
        Ok(Authorized { user_id })
    }

    async fn files(&self, user: Uuid) -> Result<Vec<proto::file_v1::File>, S3Error> {
        let mut client = FileServiceClient::new(self.grpc_channel(&self.file_endpoint).await?);
        let mut request = Request::new(ListFilesRequest {
            user_id: user.to_string(),
            folder_id: None,
        });
        let token = self.service_token().await?;
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?,
        );
        Ok(client
            .list_files(request)
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?
            .into_inner()
            .files)
    }

    async fn object_for(
        &self,
        user: Uuid,
        key: &str,
    ) -> Result<Option<proto::file_v1::File>, S3Error> {
        Ok(self
            .files(user)
            .await?
            .into_iter()
            .find(|f| f.object_key == key))
    }

    async fn create_metadata(
        &self,
        auth: &Authorized,
        key: &str,
        size: i64,
        mime: String,
        sha: String,
    ) -> Result<proto::file_v1::File, S3Error> {
        let mut client = FileServiceClient::new(self.grpc_channel(&self.file_endpoint).await?);
        let mut request = Request::new(CreateFileRequest {
            user_id: auth.user_id.to_string(),
            folder_id: None,
            name: key.rsplit('/').next().unwrap_or("object").to_owned(),
            mime_type: mime,
            size_bytes: size,
            sha256: sha,
            idempotency_key: format!(
                "s3:{}",
                Sha256::digest(key.as_bytes())
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
            object_key: Some(key.to_owned()),
        });
        let token = self.service_token().await?;
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?,
        );
        client
            .create_file(request)
            .await
            .map_err(|status| {
                tracing::warn!(%status, "file-service create_file failed");
                error(StatusCode::BAD_GATEWAY, "ServiceUnavailable")
            })?
            .into_inner()
            .file
            .ok_or_else(|| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))
    }

    async fn confirm(&self, user: Uuid, id: Uuid) -> Result<(), S3Error> {
        let mut client = FileServiceClient::new(self.grpc_channel(&self.file_endpoint).await?);
        let mut request = Request::new(ConfirmFileRequest {
            user_id: user.to_string(),
            file_id: id.to_string(),
        });
        let token = self.service_token().await?;
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?,
        );
        client
            .confirm_file(request)
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        Ok(())
    }

    async fn logical_delete(&self, user: Uuid, id: Uuid) -> Result<(), S3Error> {
        let mut client = FileServiceClient::new(self.grpc_channel(&self.file_endpoint).await?);
        let mut request = Request::new(DeleteFileRequest {
            user_id: user.to_string(),
            file_id: id.to_string(),
            reason: "s3_delete".into(),
        });
        let token = self.service_token().await?;
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError"))?,
        );
        client
            .delete_file(request)
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        Ok(())
    }

    async fn hash_object(&self, key: &str) -> Result<(i64, String), S3Error> {
        let output = self
            .r2
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        let mut reader = output.body.into_async_read();
        let mut hasher = Sha256::new();
        let mut total = 0i64;
        let mut buffer = [0u8; 1024 * 1024];
        loop {
            let read = reader
                .read(&mut buffer)
                .await
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
            if read == 0 {
                break;
            }
            total += read as i64;
            hasher.update(&buffer[..read]);
        }
        Ok((total, hex::encode(hasher.finalize())))
    }

    async fn object_content_type(&self, key: &str) -> Result<String, S3Error> {
        Ok(self
            .r2
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_owned())
    }
}

#[derive(Clone)]
struct SigV4 {
    access_key: String,
    date: String,
    region: String,
    signed_headers: Vec<String>,
    signature: String,
}

fn parse_sigv4(headers: &HeaderMap, uri: &http::Uri) -> Option<SigV4> {
    let auth = headers.get("authorization")?.to_str().ok()?;
    let rest = auth.strip_prefix("AWS4-HMAC-SHA256 ")?;
    let mut fields = HashMap::new();
    for item in rest.split(',') {
        let (k, v) = item.trim().split_once('=')?;
        fields.insert(k, v);
    }
    let credential = fields.get("Credential")?;
    let parts: Vec<_> = credential.split('/').collect();
    if parts.len() != 5 || parts[4] != "aws4_request" {
        return None;
    }
    if parts[0].is_empty()
        || parts[1].len() != 8
        || !parts[1].chars().all(|c| c.is_ascii_digit())
        || parts[2].is_empty()
    {
        return None;
    }
    let signed_headers = fields
        .get("SignedHeaders")?
        .split(';')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if signed_headers.is_empty()
        || !signed_headers.iter().all(|h| headers.contains_key(h))
        || uri.path().is_empty()
    {
        return None;
    }
    Some(SigV4 {
        access_key: parts[0].to_owned(),
        date: parts[1].to_owned(),
        region: parts[2].to_owned(),
        signed_headers,
        signature: fields.get("Signature")?.to_string(),
    })
}

fn canonical_query(uri: &http::Uri) -> String {
    let mut values = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|v| !v.is_empty())
        .map(|v| {
            let mut p = v.splitn(2, '=');
            (p.next().unwrap_or_default(), p.next().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    values.sort();
    values
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

fn verify_sigv4(
    sig: &SigV4,
    headers: &HeaderMap,
    uri: &http::Uri,
    method: &axum::http::Method,
    secret: &str,
) -> bool {
    let amz_date = match headers.get("x-amz-date").and_then(|v| v.to_str().ok()) {
        Some(v) if v.len() == 16 => v,
        _ => return false,
    };
    if !amz_date.starts_with(&sig.date) {
        return false;
    }
    let signed_at = match chrono::NaiveDateTime::parse_from_str(amz_date, "%Y%m%dT%H%M%SZ") {
        Ok(value) => value.and_utc(),
        Err(_) => return false,
    };
    if (chrono::Utc::now() - signed_at)
        .num_seconds()
        .unsigned_abs()
        > 900
    {
        return false;
    }
    let canonical_headers = sig
        .signed_headers
        .iter()
        .map(|h| {
            format!(
                "{}:{}\n",
                h,
                headers
                    .get(h)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .trim()
            )
        })
        .collect::<String>();
    let payload = headers
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("UNSIGNED-PAYLOAD");
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        uri.path(),
        canonical_query(uri),
        canonical_headers,
        sig.signed_headers.join(";"),
        payload
    );
    let hash = hex::encode(Sha256::digest(canonical.as_bytes()));
    let scope = format!("{}/{}/{}/aws4_request", sig.date, sig.region, "s3");
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{hash}");
    let mut k = hmac_bytes(format!("AWS4{secret}").as_bytes(), sig.date.as_bytes());
    k = hmac_bytes(&k, sig.region.as_bytes());
    k = hmac_bytes(&k, b"s3");
    k = hmac_bytes(&k, b"aws4_request");
    hex::encode(hmac_bytes(&k, string_to_sign.as_bytes())) == sig.signature
}

fn hmac_bytes(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary keys");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[tracing::instrument(name = "s3.request", skip(app, headers, body), fields(http.request.method = %method, url.path = %uri.path()))]
async fn handler(
    State(app): State<Arc<App>>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: http::Uri,
    method: axum::http::Method,
    body: Body,
) -> Result<Response, S3Error> {
    let started = Instant::now();
    let operation = method.as_str().to_owned();
    let result = process_request(
        State(app.clone()),
        Path((bucket, key)),
        Query(query),
        headers,
        uri,
        method,
        body,
    )
    .await;
    metrics::counter!("s3_gateway_requests_total", "operation" => operation.clone(), "status" => if result.is_ok() { "success" } else { "error" }).increment(1);
    metrics::histogram!("s3_gateway_request_duration_seconds", "operation" => operation)
        .record(started.elapsed().as_secs_f64());
    if result.is_err() {
        metrics::counter!("s3_gateway_errors_total").increment(1);
    }
    result
}

async fn process_request(
    State(app): State<Arc<App>>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: http::Uri,
    method: axum::http::Method,
    body: Body,
) -> Result<Response, S3Error> {
    if bucket != app.bucket || key.is_empty() || key.len() > 1024 || key.contains("..") {
        return Err(error(StatusCode::NOT_FOUND, "NoSuchKey"));
    }
    let is_multipart_init = method == axum::http::Method::POST && query.contains_key("uploads");
    let is_part = query.contains_key("partNumber") && query.contains_key("uploadId");
    let scope = if method == axum::http::Method::GET {
        "files:read"
    } else if method == axum::http::Method::DELETE {
        "files:delete"
    } else {
        "files:write"
    };
    let auth = app.authorize(&headers, scope, &uri, &method).await?;
    if !key.starts_with(&format!("s3/{}/", auth.user_id)) {
        return Err(error(StatusCode::FORBIDDEN, "AccessDenied"));
    }
    if is_multipart_init {
        let out = app
            .r2
            .create_multipart_upload()
            .bucket(&app.bucket)
            .key(&key)
            .content_type(
                headers
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/octet-stream"),
            )
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        return Ok((StatusCode::OK, format!("<?xml version=\"1.0\"?><InitiateMultipartUploadResult><Bucket>{bucket}</Bucket><Key>{key}</Key><UploadId>{}</UploadId></InitiateMultipartUploadResult>", out.upload_id().unwrap_or_default())).into_response());
    }
    if is_part && method == axum::http::Method::PUT {
        let upload = query.get("uploadId").cloned().unwrap();
        let part: i32 = query
            .get("partNumber")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "InvalidPart"))?;
        let bytes = to_bytes(body, app.max_part_size)
            .await
            .map_err(|_| error(StatusCode::PAYLOAD_TOO_LARGE, "EntityTooLarge"))?;
        let out = app
            .r2
            .upload_part()
            .bucket(&app.bucket)
            .key(&key)
            .upload_id(upload)
            .part_number(part)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        let etag = out.e_tag().unwrap_or_default();
        return Ok((
            StatusCode::OK,
            [(
                "etag",
                HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("\"\"")),
            )],
            "",
        )
            .into_response());
    }
    if method == axum::http::Method::POST && query.contains_key("uploadId") {
        let upload = query.get("uploadId").cloned().unwrap();
        let data = to_bytes(body, 2 * 1024 * 1024)
            .await
            .map_err(|_| error(StatusCode::BAD_REQUEST, "MalformedXML"))?;
        let xml = String::from_utf8_lossy(&data);
        let mut parts = Vec::new();
        for item in xml.split("<Part>").skip(1) {
            let num = item
                .split("<PartNumber>")
                .nth(1)
                .and_then(|v| v.split("</PartNumber>").next())
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "InvalidPart"))?;
            let etag = item
                .split("<ETag>")
                .nth(1)
                .and_then(|v| v.split("</ETag>").next())
                .unwrap_or_default()
                .trim_matches('"')
                .to_owned();
            parts.push(
                CompletedPart::builder()
                    .part_number(num)
                    .e_tag(etag)
                    .build(),
            );
        }
        if parts.is_empty() {
            return Err(error(StatusCode::BAD_REQUEST, "InvalidPart"));
        }
        parts.sort_by_key(|p| p.part_number().unwrap_or_default());
        app.r2
            .complete_multipart_upload()
            .bucket(&app.bucket)
            .key(&key)
            .upload_id(upload)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        let (size, sha) = app.hash_object(&key).await?;
        let mime = app.object_content_type(&key).await?;
        let file = app.create_metadata(&auth, &key, size, mime, sha).await?;
        app.confirm(
            auth.user_id,
            Uuid::parse_str(&file.id)
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?,
        )
        .await?;
        return Ok((StatusCode::OK, format!("<?xml version=\"1.0\"?><CompleteMultipartUploadResult><Bucket>{bucket}</Bucket><Key>{key}</Key></CompleteMultipartUploadResult>")).into_response());
    }
    if method == axum::http::Method::GET {
        let file = app
            .object_for(auth.user_id, &key)
            .await?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "NoSuchKey"))?;
        let mut req = app.r2.get_object().bucket(&app.bucket).key(&key);
        if let Some(range) = headers.get("range").and_then(|v| v.to_str().ok()) {
            req = req.range(range);
        }
        let out = req
            .send()
            .await
            .map_err(|_| error(StatusCode::NOT_FOUND, "NoSuchKey"))?;
        let content_type = out.content_type().map(str::to_owned);
        let content_length = out.content_length();
        let etag = out.e_tag().map(str::to_owned);
        let status = if headers.contains_key("range") {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        };
        let mut response = (
            status,
            Body::from_stream(ReaderStream::new(out.body.into_async_read())),
        )
            .into_response();
        if let Some(v) = content_type {
            response
                .headers_mut()
                .insert("content-type", HeaderValue::from_str(&v).unwrap());
        }
        if let Some(v) = content_length {
            response
                .headers_mut()
                .insert("content-length", HeaderValue::from(v));
        }
        if let Some(v) = etag {
            response
                .headers_mut()
                .insert("etag", HeaderValue::from_str(&v).unwrap());
        }
        response
            .headers_mut()
            .insert("x-s3-user", HeaderValue::from_str(&file.user_id).unwrap());
        return Ok(response);
    }
    if method == axum::http::Method::PUT {
        let bytes = to_bytes(body, app.max_single_upload)
            .await
            .map_err(|_| error(StatusCode::PAYLOAD_TOO_LARGE, "EntityTooLarge"))?;
        let sha = hex::encode(Sha256::digest(&bytes));
        let mime = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let file = match app.object_for(auth.user_id, &key).await? {
            Some(file) => file,
            None => {
                app.create_metadata(&auth, &key, bytes.len() as i64, mime.clone(), sha.clone())
                    .await?
            }
        };
        app.r2
            .put_object()
            .bucket(&app.bucket)
            .key(&key)
            .content_type(mime)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        app.confirm(
            auth.user_id,
            Uuid::parse_str(&file.id)
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?,
        )
        .await?;
        return Ok((StatusCode::OK, [("etag", "\"s3-gateway\"")], "").into_response());
    }
    if method == axum::http::Method::DELETE {
        if let Some(file) = app.object_for(auth.user_id, &key).await? {
            app.logical_delete(
                auth.user_id,
                Uuid::parse_str(&file.id)
                    .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?,
            )
            .await?;
        }
        app.r2
            .delete_object()
            .bucket(&app.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|_| error(StatusCode::BAD_GATEWAY, "ServiceUnavailable"))?;
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    Err(error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed"))
}

async fn bucket_handler(
    State(app): State<Arc<App>>,
    Path(bucket): Path<String>,
    Query(_query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    uri: http::Uri,
) -> Result<Response, S3Error> {
    if bucket != app.bucket {
        return Err(error(StatusCode::NOT_FOUND, "NoSuchBucket"));
    }
    let auth = app
        .authorize(&headers, "files:read", &uri, &axum::http::Method::GET)
        .await?;
    let prefix = format!("s3/{}/", auth.user_id);
    let files = app.files(auth.user_id).await?;
    let contents = files
        .into_iter()
        .filter(|f| f.object_key.starts_with(&prefix))
        .map(|f| format!("<Contents><Key>{}</Key><Size>{}</Size><ETag>&quot;{}&quot;</ETag><StorageClass>STANDARD</StorageClass></Contents>", xml_escape(&f.object_key[prefix.len()..]), f.size_bytes, f.sha256))
        .collect::<String>();
    let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult><Name>{bucket}</Name>{contents}</ListBucketResult>");
    Ok((StatusCode::OK, [("content-type", "application/xml")], body).into_response())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

async fn metrics_handler(State(app): State<Arc<App>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        app.metrics.render(),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    std::mem::drop(config_reload);
    common::observability::init("s3-gateway", &config)
        .context("failed to initialize observability")?;
    let tls =
        ServiceTlsConfig::from_env("s3-gateway").context("invalid s3-gateway TLS configuration")?;
    let endpoint = std::env::var("FILE_SERVICE_R2_ENDPOINT_URL")
        .or_else(|_| std::env::var("INVOICE_R2_ENDPOINT_URL"))?;
    let access = std::env::var("FILE_SERVICE_R2_ACCESS_KEY_ID")
        .or_else(|_| std::env::var("INVOICE_R2_ACCESS_KEY_ID"))?;
    let secret = std::env::var("FILE_SERVICE_R2_SECRET_ACCESS_KEY")
        .or_else(|_| std::env::var("INVOICE_R2_SECRET_ACCESS_KEY"))?;
    let bucket =
        std::env::var("FILE_SERVICE_R2_BUCKET").or_else(|_| std::env::var("INVOICE_R2_BUCKET"))?;
    let credentials = Credentials::new(access, secret, None, None, "vault");
    let r2 = aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::config::Builder::new()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("auto"))
            .endpoint_url(endpoint)
            .credentials_provider(credentials)
            .force_path_style(true)
            .build(),
    );
    let max_single_upload = std::env::var("S3_GATEWAY_MAX_UPLOAD_BYTES")
        .unwrap_or_else(|_| DEFAULT_MAX_UPLOAD.to_string())
        .parse::<usize>()
        .context("S3_GATEWAY_MAX_UPLOAD_BYTES must be a positive integer")?;
    let max_part_size = std::env::var("S3_GATEWAY_MAX_PART_BYTES")
        .unwrap_or_else(|_| DEFAULT_MAX_UPLOAD.to_string())
        .parse::<usize>()
        .context("S3_GATEWAY_MAX_PART_BYTES must be a positive integer")?;
    if max_single_upload == 0 || max_part_size == 0 {
        anyhow::bail!("S3 gateway upload limits must be greater than zero");
    }
    let metrics = PrometheusBuilder::new()
        .install_recorder()
        .context("failed to install s3-gateway metrics recorder")?;
    let app = Arc::new(App {
        r2,
        bucket,
        key_endpoint: std::env::var("KEY_SERVICES_GRPC_ENDPOINT")?,
        file_endpoint: std::env::var("FILE_SERVICE_GRPC_ENDPOINT")
            .unwrap_or_else(|_| "https://127.0.0.1:50071".into()),
        jwt_secret: config
            .service_jwt_secret
            .context("SERVICE_JWT_SECRET required")?,
        jwt_issuer: std::env::var("S3_GATEWAY_SERVICE_JWT_ISSUER")
            .unwrap_or_else(|_| "s3-gateway".into()),
        jwt_subject: std::env::var("S3_GATEWAY_SERVICE_JWT_SUBJECT")
            .unwrap_or_else(|_| "s3-gateway".into()),
        tls,
        max_single_upload,
        max_part_size,
        metrics,
    });
    let bind = std::env::var("S3_GATEWAY_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".into());
    let port: u16 = std::env::var("S3_GATEWAY_PORT")
        .unwrap_or_else(|_| "4002".into())
        .parse()?;
    let addr: SocketAddr = format!("{bind}:{port}").parse()?;
    tracing::info!(%addr, "s3-gateway listening");
    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/{bucket}", any(bucket_handler))
            .route("/{bucket}/{*key}", any(handler))
            .with_state(app),
    )
    .await?;
    Ok(())
}
