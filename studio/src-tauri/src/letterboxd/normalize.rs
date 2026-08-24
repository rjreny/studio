pub fn decode_html_entities(value: &str) -> String {
    value
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

pub fn normalize_title(value: &str) -> String {
    decode_html_entities(value).trim().to_lowercase()
}

pub fn parse_year(value: &str) -> Option<i32> {
    value.trim().parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_apostrophe_entity() {
        assert_eq!(normalize_title("Pan&#039;s Labyrinth"), "pan's labyrinth");
    }
}
