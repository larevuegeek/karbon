use crate::error::AppError;
use validator::Validate;

/// Validate a request/input struct and return a formatted AppError if invalid
pub fn validate_request<T: Validate>(input: &T) -> Result<(), AppError> {
    input.validate().map_err(|errors| {
        let messages: Vec<String> = errors
            .field_errors()
            .iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |e| {
                    let msg = e
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| format!("Invalid value for {}", field));
                    format!("{}: {}", field, msg)
                })
            })
            .collect();

        AppError::Validation(messages.join(", "))
    })
}

/// Alias for backward compatibility
#[inline]
pub fn validate_input<T: Validate>(input: &T) -> Result<(), AppError> {
    validate_request(input)
}