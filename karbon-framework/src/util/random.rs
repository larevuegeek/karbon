use rand::RngExt;

/// Random value generation helpers
pub struct RandomHelper;

impl RandomHelper {
    /// Generate a random hex token of given byte length
    /// e.g., token(16) => "a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6" (32 hex chars)
    pub fn token(byte_length: usize) -> String {
        let mut rng = rand::rng();
        let bytes: Vec<u8> = (0..byte_length).map(|_| rng.random()).collect();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Generate a random alphanumeric string of given length
    pub fn alphanumeric(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        (0..length)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate a numeric OTP code of given length
    /// e.g., otp(6) => "482951"
    pub fn otp(length: usize) -> String {
        let mut rng = rand::rng();
        (0..length)
            .map(|_| {
                let digit: u8 = rng.random_range(0..10);
                char::from(b'0' + digit)
            })
            .collect()
    }

    /// Generate a random integer within a range (inclusive)
    pub fn int(min: i64, max: i64) -> i64 {
        let mut rng = rand::rng();
        rng.random_range(min..=max)
    }

    /// Pick a random element from a slice
    pub fn pick<'a, T>(items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let mut rng = rand::rng();
        let idx = rng.random_range(0..items.len());
        Some(&items[idx])
    }

    /// Shuffle a vector in place and return it
    pub fn shuffle<T>(mut items: Vec<T>) -> Vec<T> {
        let mut rng = rand::rng();
        let len = items.len();
        for i in (1..len).rev() {
            let j = rng.random_range(0..=i);
            items.swap(i, j);
        }
        items
    }

    /// Generate a UUID v4 string
    pub fn uuid() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_length() {
        let t = RandomHelper::token(16);
        assert_eq!(t.len(), 32); // 16 bytes = 32 hex chars
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_token_uniqueness() {
        let a = RandomHelper::token(16);
        let b = RandomHelper::token(16);
        assert_ne!(a, b);
    }

    #[test]
    fn test_alphanumeric_length() {
        let s = RandomHelper::alphanumeric(20);
        assert_eq!(s.len(), 20);
        assert!(s.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_otp_length_and_digits() {
        let code = RandomHelper::otp(6);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_otp_4_digits() {
        let code = RandomHelper::otp(4);
        assert_eq!(code.len(), 4);
    }

    #[test]
    fn test_int_range() {
        for _ in 0..100 {
            let n = RandomHelper::int(1, 10);
            assert!((1..=10).contains(&n));
        }
    }

    #[test]
    fn test_pick() {
        let items = vec!["a", "b", "c"];
        let picked = RandomHelper::pick(&items);
        assert!(picked.is_some());
        assert!(items.contains(picked.unwrap()));
    }

    #[test]
    fn test_pick_empty() {
        let items: Vec<i32> = vec![];
        assert!(RandomHelper::pick(&items).is_none());
    }

    #[test]
    fn test_shuffle_preserves_elements() {
        let items = vec![1, 2, 3, 4, 5];
        let shuffled = RandomHelper::shuffle(items.clone());
        assert_eq!(shuffled.len(), items.len());
        for item in &items {
            assert!(shuffled.contains(item));
        }
    }

    #[test]
    fn test_uuid_format() {
        let id = RandomHelper::uuid();
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|&c| c == '-').count(), 4);
    }
}
