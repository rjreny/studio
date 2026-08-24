use sha2::{Digest, Sha256};

pub const FINGERPRINT_VERSION: &str = "v1";

pub fn source_record_key(source_type: &str, dataset: &str, fingerprint: &str) -> String {
    format!("{source_type}|{dataset}|{FINGERPRINT_VERSION}|{fingerprint}")
}

pub fn row_fingerprint(fields: &[(&str, &str)]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_VERSION.as_bytes());
    hasher.update(b"|");
    for (key, value) in fields {
        hasher.update(key.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
        hasher.update(b"|");
    }
    hex::encode(hasher.finalize())
}

pub fn content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_fields_same_fingerprint() {
        let fields = [
            ("name", "Inception"),
            ("year", "2010"),
            ("date", "2020-01-01"),
        ];
        let a = row_fingerprint(&fields);
        let b = row_fingerprint(&fields);
        assert_eq!(a, b);
    }

    #[test]
    fn different_zip_hash_same_row_key() {
        let fields = [
            ("name", "Inception"),
            ("year", "2010"),
            ("date", "2020-01-01"),
        ];
        let fp = row_fingerprint(&fields);
        let key_a = source_record_key("letterboxd_export", "diary.csv", &fp);
        let key_b = source_record_key("letterboxd_export", "diary.csv", &fp);
        assert_eq!(key_a, key_b);
    }
}
