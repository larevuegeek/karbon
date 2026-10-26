//! `Idempotency-Key` header (Stripe convention): reading and validation only. The
//! deduplication itself belongs to the storage (a unique column, a lookup before insert).

use axum::http::HeaderMap;

use crate::error::AppError;

pub const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// 8 to 64 characters `[a-zA-Z0-9-_]`. No UUID enforced: the client picks its scheme,
/// the only expectation is uniqueness on its side.
pub fn validate_idempotency_key(raw: &str) -> Result<&str, AppError> {
    let key = raw.trim();

    if key.len() < 8 || key.len() > 64 {
        return Err(AppError::BadRequest(
            "Idempotency-Key doit faire entre 8 et 64 caractères.".into(),
        ));
    }

    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest(
            "Idempotency-Key : caractères autorisés [a-zA-Z0-9-_].".into(),
        ));
    }

    Ok(key)
}

/// `Ok(None)` when the header is absent, `Err` when it is present but malformed.
pub fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, AppError> {
    let Some(value) = headers.get(IDEMPOTENCY_KEY_HEADER) else {
        return Ok(None);
    };

    let raw = value
        .to_str()
        .map_err(|_| AppError::BadRequest("Idempotency-Key non-ASCII.".into()))?;

    validate_idempotency_key(raw).map(|key| Some(key.to_string()))
}

/// For endpoints where a replay must never have two effects (payments…).
pub fn require_idempotency_key(headers: &HeaderMap) -> Result<String, AppError> {
    idempotency_key(headers)?.ok_or_else(|| {
        AppError::BadRequest("Header Idempotency-Key obligatoire sur cet endpoint.".into())
    })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            IDEMPOTENCY_KEY_HEADER,
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    #[test]
    fn accepts_reasonable_keys() {
        assert!(validate_idempotency_key("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert_eq!(
            idempotency_key(&headers("  order-1234_abcd  "))
                .unwrap()
                .as_deref(),
            Some("order-1234_abcd")
        );
    }

    #[test]
    fn rejects_short_or_odd_keys() {
        for key in [
            "short",
            "key with spaces",
            "key/with/slashes",
            "key;DROP TABLE",
        ] {
            assert!(validate_idempotency_key(key).is_err(), "{key}");
        }
    }

    #[test]
    fn absent_is_none_unless_required() {
        assert_eq!(idempotency_key(&HeaderMap::new()).unwrap(), None);
        assert!(require_idempotency_key(&HeaderMap::new()).is_err());
    }
}
