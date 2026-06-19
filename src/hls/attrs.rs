use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeList {
    map: HashMap<String, String>,
}

impl AttributeList {
    pub fn parse(input: &str) -> Self {
        let mut map = HashMap::new();
        let bytes = input.as_bytes();
        let mut i = 0usize;
        let len = bytes.len();

        while i < len {
            while i < len && (bytes[i] as char).is_whitespace() {
                i += 1;
            }
            if i >= len {
                break;
            }

            let name_start = i;
            while i < len && bytes[i] != b'=' {
                i += 1;
            }
            if i >= len {
                break;
            }
            let name = input[name_start..i].trim().to_string();
            i += 1;

            if i >= len {
                map.insert(name, String::new());
                break;
            }

            let value: String;
            if bytes[i] == b'"' {
                i += 1;
                let val_start = i;
                while i < len && bytes[i] != b'"' {
                    i += 1;
                }
                value = input[val_start..i].to_string();
                if i < len {
                    i += 1;
                }
            } else {
                let val_start = i;
                while i < len && bytes[i] != b',' {
                    i += 1;
                }
                value = input[val_start..i].trim().to_string();
            }

            map.insert(name, value);

            while i < len && (bytes[i] == b',' || (bytes[i] as char).is_whitespace()) {
                i += 1;
            }
        }

        AttributeList { map }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.get(key)?.parse().ok()
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.parse().ok()
    }

    pub fn get_resolution(&self, key: &str) -> Option<(u32, u32)> {
        let raw = self.get(key)?;
        let (w, h) = raw.split_once('x').or_else(|| raw.split_once('X'))?;
        Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_attributes() {
        let attrs = AttributeList::parse(r#"BANDWIDTH=1280000,RESOLUTION=1920x1080"#);
        assert_eq!(attrs.get_u64("BANDWIDTH"), Some(1_280_000));
        assert_eq!(attrs.get_resolution("RESOLUTION"), Some((1920, 1080)));
    }

    #[test]
    fn handles_quoted_values_with_commas() {
        let attrs = AttributeList::parse(
            r#"BANDWIDTH=300000,CODECS="avc1.64001f,mp4a.40.2",AUDIO="aud-lo""#,
        );
        assert_eq!(attrs.get("CODECS"), Some("avc1.64001f,mp4a.40.2"));
        assert_eq!(attrs.get("AUDIO"), Some("aud-lo"));
        assert_eq!(attrs.get_u64("BANDWIDTH"), Some(300_000));
    }

    #[test]
    fn handles_quoted_uri_and_trailing_attrs() {
        let attrs = AttributeList::parse(r#"TYPE=AUDIO,URI="audio/lo.m3u8",DEFAULT=YES"#);
        assert_eq!(attrs.get("TYPE"), Some("AUDIO"));
        assert_eq!(attrs.get("URI"), Some("audio/lo.m3u8"));
        assert_eq!(attrs.get("DEFAULT"), Some("YES"));
    }
}
