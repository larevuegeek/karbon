use axum::{
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Once the map grows past this many keys, expired windows are swept out. Bounds the
/// memory a flood of distinct IPs (e.g. spoofed IPv6) can consume.
const SWEEP_THRESHOLD: usize = 20_000;
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
    trusted_proxies: Arc<Vec<String>>,
}

impl RateLimitLayer {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            store: Arc::new(Mutex::new(HashMap::new())),
            trusted_proxies: Arc::new(Vec::new()),
        }
    }

    /// Per-minute shorthand
    pub fn per_minute(max: u32) -> Self {
        Self::new(max, Duration::from_secs(60))
    }

    /// Trust `X-Forwarded-For` only when the direct peer is one of these proxy IPs
    /// (or `*`). Without this, the socket peer IP is used (safe when Karbon is the edge).
    pub fn trust_proxies(mut self, proxies: Vec<String>) -> Self {
        self.trusted_proxies = Arc::new(proxies);
        self
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
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    max_requests: u32,
    window: Duration,
    store: Arc<Mutex<HashMap<IpAddr, (u32, Instant)>>>,
    trusted_proxies: Arc<Vec<String>>,
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
        let trusted = self.trusted_proxies.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Use the socket peer IP by default (unspoofable). If that peer is a configured
            // trusted proxy, resolve the real client from X-Forwarded-For instead — so
            // rate-limiting is correct behind nginx/Cloudflare yet safe when Karbon is edge.
            let peer: IpAddr = request
                .extensions()
                .get::<axum::extract::ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip())
                .unwrap_or_else(|| "127.0.0.1".parse().unwrap());
            let ip: IpAddr =
                crate::util::HttpHelper::client_ip_trusted(request.headers(), peer, &trusted);

            let mut map = store.lock().await;
            let now = Instant::now();

            // Evict expired windows once the map gets large (bounded memory).
            if map.len() > SWEEP_THRESHOLD {
                map.retain(|_, (_, started)| now.duration_since(*started) <= window);
            }

            let (count, started) = map.entry(ip).or_insert((0, now));

            // Reset window if expired
            if now.duration_since(*started) > window {
                *count = 0;
                *started = now;
            }

            *count += 1;

            if *count > max {
                drop(map);
                return Ok((StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response());
            }
            drop(map);

            inner.call(request).await
        })
    }
}
