pub fn resolve(base: Option<&str>, reference: &str) -> String {
    let reference = reference.trim();

    if is_absolute(reference) {
        return reference.to_string();
    }

    let base = match base {
        Some(b) => b,
        None => return reference.to_string(),
    };

    if let Some(rest) = reference.strip_prefix("//") {
        if let Some(scheme) = scheme_of(base) {
            return format!("{}://{}", scheme, rest);
        }
        return reference.to_string();
    }

    let (scheme, authority, base_path) = match split_url(base) {
        Some(parts) => parts,
        None => return reference.to_string(),
    };

    if let Some(rest) = reference.strip_prefix('/') {
        return format!("{}://{}/{}", scheme, authority, rest);
    }

    let base_dir = match base_path.rfind('/') {
        Some(idx) => &base_path[..=idx],
        None => "/",
    };

    let combined = format!("{}{}", base_dir, reference);
    let normalized = normalize_path(&combined);

    format!("{}://{}{}", scheme, authority, normalized)
}

fn is_absolute(s: &str) -> bool {
    if let Some(colon_idx) = s.find(':') {
        let candidate = &s[..colon_idx];
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && candidate.chars().next().unwrap().is_ascii_alphabetic()
        {
            return true;
        }
    }
    false
}

fn scheme_of(url: &str) -> Option<&str> {
    let idx = url.find("://")?;
    Some(&url[..idx])
}

fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    let rest = &url[scheme_end + 3..];
    let path_start = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..path_start];
    let path = &rest[path_start..];
    let path = if path.is_empty() { "/" } else { path };
    Some((scheme, authority, path))
}

fn normalize_path(path: &str) -> String {
    let (path_only, suffix) = match path.find(['?', '#']) {
        Some(idx) => (&path[..idx], &path[idx..]),
        None => (path, ""),
    };

    let mut out: Vec<&str> = Vec::new();
    for seg in path_only.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }

    format!("/{}{}", out.join("/"), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_segment() {
        let r = resolve(
            Some("https://cdn.example.com/videos/master.m3u8"),
            "720p/segment0001.ts",
        );
        assert_eq!(r, "https://cdn.example.com/videos/720p/segment0001.ts");
    }

    #[test]
    fn resolves_parent_relative_segment() {
        let r = resolve(
            Some("https://cdn.example.com/videos/720p/index.m3u8"),
            "../audio/lo/seg1.aac",
        );
        assert_eq!(r, "https://cdn.example.com/videos/audio/lo/seg1.aac");
    }

    #[test]
    fn leaves_absolute_urls_untouched() {
        let r = resolve(
            Some("https://cdn.example.com/videos/master.m3u8"),
            "https://other-cdn.example.com/seg.ts",
        );
        assert_eq!(r, "https://other-cdn.example.com/seg.ts");
    }

    #[test]
    fn resolves_root_relative() {
        let r = resolve(
            Some("https://cdn.example.com/a/b/master.m3u8"),
            "/segments/seg1.ts",
        );
        assert_eq!(r, "https://cdn.example.com/segments/seg1.ts");
    }

    #[test]
    fn no_base_returns_reference_as_is() {
        let r = resolve(None, "segment0001.ts");
        assert_eq!(r, "segment0001.ts");
    }
}
