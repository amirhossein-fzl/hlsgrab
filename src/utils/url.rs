use reqwest::Url;

pub fn filename_from_url(url: &Url) -> Option<String> {
    if url.path().ends_with("/") {
        return None;
    }

    url.path_segments()?
        .filter(|segment| !segment.is_empty())
        .next_back()
        .map(|segment| {
            percent_encoding::percent_decode_str(segment)
                .decode_utf8_lossy()
                .into_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let url = Url::parse("https://example.com/path/to/file.zip").unwrap();
        assert_eq!(filename_from_url(&url), Some("file.zip".to_string()));
    }

    #[test]
    fn with_query_and_fragment() {
        let url = Url::parse("https://example.com/a/b/report.pdf?x=1#top").unwrap();
        assert_eq!(filename_from_url(&url), Some("report.pdf".to_string()));
    }

    #[test]
    fn trailing_slash() {
        let url = Url::parse("https://example.com/path/to/dir/").unwrap();
        assert_eq!(filename_from_url(&url), None);
    }

    #[test]
    fn no_path() {
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(filename_from_url(&url), None);
    }

    #[test]
    fn percent_encoded() {
        let url = Url::parse("https://example.com/my%20file.txt").unwrap();
        assert_eq!(filename_from_url(&url), Some("my file.txt".to_string()));
    }
}
