use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tower::{Layer, Service};

/// Simple in-memory rate limiter (per IP address).
///
/// # Security note
/// Uses `X-Forwarded-For` / `X-Real-IP` headers to identify clients.
/// **These headers can be spoofed** unless your reverse proxy (nginx, Cloudflare, etc.)
/// strips and re-sets them. In production, always place this behind a trusted reverse proxy
/// that sets `X-Forwarded-For` to the real client IP.
///
/// For distributed systems, consider a Redis-backed rate limiter instead.
///
/// # Example
/// ```ignore
/// let app = Router::new()
///     .route("/api/login", post(login))
///     .layer(RateLimitLayer::new(60, Duration::from_secs(60))); // 60 req/min
/// ```
#[derive(Clone)]
pub struct RateLimitLayer {
    max_requests: u32,
    window: Duration,
    store: Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>,
}

impl RateLimitLayer {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Per-minute shorthand
    pub fn per_minute(max: u32) -> Self {
        Self::new(max, Duration::from_secs(60))
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            max_requests: self.max_requests,
            window: self.window,
            store: self.store.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    max_requests: u32,
    window: Duration,
    store: Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>,
}

impl<S> Service<Request> for RateLimitService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let max = self.max_requests;
        let window = self.window;
        let store = self.store.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract IP from proxy headers or fallback to loopback
            let ip: IpAddr = crate::util::HttpHelper::client_ip(
                request.headers(),
                "127.0.0.1".parse().unwrap(),
            );

            let mut map = store.lock().await;
            let now = Instant::now();

            let (count, started) = map.entry(ip).or_insert((0, now));

            // Reset window if expired
            if now.duration_since(*started) > window {
                *count = 0;
                *started = now;
            }

            *count += 1;

            if *count > max {
                drop(map);
                return Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    "Rate limit exceeded",
                )
                    .into_response());
            }
            drop(map);

            inner.call(request).await
        })
    }
}
