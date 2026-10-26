use axum::response::{IntoResponse, Response};

/// Response for a handler that panicked.
///
/// Without it the connection is dropped with no HTTP response at all, and a panic is easy
/// to reach from outside: `SelectBuilder` rejects an invalid identifier with `assert!`, so
/// a `?sort=` value passed to `order_by` panics on purpose. Going through `AppError` logs
/// the panic and masks its message outside development — the message can quote the
/// request that triggered it.
pub fn panic_response(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    let detail = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload");
    crate::error::AppError::Internal(format!("handler panicked: {detail}")).into_response()
}
