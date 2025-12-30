use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// HTTP method enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            "PATCH" => Ok(Self::Patch),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            _ => Err(()),
        }
    }
}

impl Default for HttpMethod {
    fn default() -> Self {
        Self::Get
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// HTTP Request data (shared type for all runtimes)
#[derive(Debug)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: RequestBody,
}

/// Request body - always buffered (no streaming input supported)
///
/// Streaming input is intentionally not supported because:
/// - 99% of requests are small JSON payloads
/// - HTTP servers (like actix) buffer the body before passing to workers
/// - Supporting streaming input adds significant complexity to runtimes
#[derive(Debug, Clone)]
pub enum RequestBody {
    /// No body
    None,
    /// Complete body (already buffered)
    Bytes(Bytes),
}

impl RequestBody {
    /// Check if this is an empty body
    pub fn is_none(&self) -> bool {
        matches!(self, RequestBody::None)
    }

    /// Check if this body has content
    pub fn is_bytes(&self) -> bool {
        matches!(self, RequestBody::Bytes(_))
    }

    /// Get bytes reference if present
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            RequestBody::Bytes(b) => Some(b),
            RequestBody::None => None,
        }
    }

    /// Convert to Option<Bytes>, consuming self
    pub fn into_bytes(self) -> Option<Bytes> {
        match self {
            RequestBody::Bytes(b) => Some(b),
            RequestBody::None => None,
        }
    }
}

/// Response body - supports streaming for SSE, chunked responses, etc.
pub enum ResponseBody {
    /// No body
    None,
    /// Complete body (already buffered)
    Bytes(Bytes),
    /// Streaming body - receiver yields chunks as they become available
    /// Uses bounded channel for backpressure support
    Stream(mpsc::Receiver<Result<Bytes, String>>),
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseBody::None => write!(f, "None"),
            ResponseBody::Bytes(b) => write!(f, "Bytes({} bytes)", b.len()),
            ResponseBody::Stream(_) => write!(f, "Stream(...)"),
        }
    }
}

impl ResponseBody {
    /// Check if this is an empty body
    pub fn is_none(&self) -> bool {
        matches!(self, ResponseBody::None)
    }

    /// Check if this is a streaming body
    pub fn is_stream(&self) -> bool {
        matches!(self, ResponseBody::Stream(_))
    }

    /// Collect all bytes from the body, consuming it.
    /// Works for both Bytes and Stream variants.
    pub async fn collect(self) -> Option<Bytes> {
        match self {
            ResponseBody::None => None,
            ResponseBody::Bytes(b) => Some(b),
            ResponseBody::Stream(mut rx) => {
                let mut chunks = Vec::new();
                while let Some(result) = rx.recv().await {
                    if let Ok(bytes) = result {
                        chunks.push(bytes);
                    }
                }
                if chunks.is_empty() {
                    None
                } else {
                    let total: Vec<u8> = chunks.iter().flat_map(|b| b.to_vec()).collect();
                    Some(Bytes::from(total))
                }
            }
        }
    }
}

/// HTTP Response data (shared type for all runtimes)
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
}

pub type ResponseSender = tokio::sync::oneshot::Sender<HttpResponse>;

/// HTTP Response metadata (for streaming responses - body comes separately)
#[derive(Debug, Clone)]
pub struct HttpResponseMeta {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
}

// Actix-web conversions (only available with actix feature)
#[cfg(feature = "actix")]
impl HttpRequest {
    /// Convert from actix_web::HttpRequest + body bytes
    pub fn from_actix(req: &actix_web::HttpRequest, body: Bytes) -> Self {
        let method = req.method().as_str().parse().unwrap_or_default();
        let url = format!(
            "{}://{}{}",
            req.connection_info().scheme(),
            req.connection_info().host(),
            req.uri()
        );

        let mut headers = HashMap::new();
        for (key, value) in req.headers() {
            if let Ok(val_str) = value.to_str() {
                headers.insert(key.to_string(), val_str.to_string());
            }
        }

        HttpRequest {
            method,
            url,
            headers,
            body: if body.is_empty() {
                RequestBody::None
            } else {
                RequestBody::Bytes(body)
            },
        }
    }
}

#[cfg(feature = "actix")]
impl From<HttpResponse> for actix_web::HttpResponse {
    fn from(res: HttpResponse) -> Self {
        use actix_web::body::BodyStream;
        use tokio_stream::StreamExt;
        use tokio_stream::wrappers::ReceiverStream;

        let mut builder = actix_web::HttpResponse::build(
            actix_web::http::StatusCode::from_u16(res.status)
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
        );

        for (key, value) in res.headers {
            builder.insert_header((key.as_str(), value.as_str()));
        }

        match res.body {
            ResponseBody::None => builder.finish(),
            ResponseBody::Bytes(body) => {
                if body.is_empty() {
                    builder.finish()
                } else {
                    builder.body(body)
                }
            }
            ResponseBody::Stream(rx) => {
                let stream = ReceiverStream::new(rx).map(|result| {
                    result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                });
                builder.body(BodyStream::new(stream))
            }
        }
    }
}
