use crate::downloader::error::DownloaderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyScheme {
    Http,
    Socks5,
}

impl ProxyScheme {
    fn as_url_prefix(self) -> &'static str {
        match self {
            ProxyScheme::Http => "http://",
            ProxyScheme::Socks5 => "socks5://",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub scheme: ProxyScheme,
    pub host: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    pub fn parse(raw: &str) -> Result<Self, DownloaderError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(DownloaderError::InvalidProxy(
                raw.to_string(),
                "proxy string is empty".into(),
            ));
        }

        let (scheme, rest) = if let Some(stripped) = raw.strip_prefix("http://") {
            (ProxyScheme::Http, stripped)
        } else if let Some(stripped) = raw.strip_prefix("https://") {
            (ProxyScheme::Http, stripped)
        } else if let Some(stripped) = raw.strip_prefix("socks5://") {
            (ProxyScheme::Socks5, stripped)
        } else if let Some(stripped) = raw.strip_prefix("socks5h://") {
            (ProxyScheme::Socks5, stripped)
        } else {
            (ProxyScheme::Http, raw)
        };

        if rest.is_empty() {
            return Err(DownloaderError::InvalidProxy(
                raw.to_string(),
                "missing host after scheme".into(),
            ));
        }

        let (auth, host_port) = match rest.rfind('@') {
            Some(idx) => (Some(&rest[..idx]), &rest[idx + 1..]),
            None => (None, rest),
        };

        let (username, password) = match auth {
            Some(auth_str) => match auth_str.split_once(':') {
                Some((u, p)) => (Some(u.to_string()), Some(p.to_string())),
                None => (Some(auth_str.to_string()), None),
            },
            None => (None, None),
        };

        if host_port.is_empty() {
            return Err(DownloaderError::InvalidProxy(
                raw.to_string(),
                "missing host".into(),
            ));
        }

        let (host, port) = split_host_port(host_port)
            .map_err(|e| DownloaderError::InvalidProxy(raw.to_string(), e))?;

        if host.is_empty() {
            return Err(DownloaderError::InvalidProxy(
                raw.to_string(),
                "missing host".into(),
            ));
        }

        Ok(ProxyConfig {
            scheme,
            host,
            port,
            username,
            password,
        })
    }

    pub fn proxy_url(&self) -> String {
        let mut url = String::from(self.scheme.as_url_prefix());
        url.push_str(&self.host);
        if let Some(port) = self.port {
            url.push(':');
            url.push_str(&port.to_string());
        }
        url
    }

    pub fn has_credentials(&self) -> bool {
        self.username.is_some()
    }

    pub fn into_reqwest_proxy(self) -> Result<reqwest::Proxy, DownloaderError> {
        let mut proxy = reqwest::Proxy::all(self.proxy_url())
            .map_err(|e| DownloaderError::InvalidProxy(self.proxy_url(), e.to_string()))?;

        if let Some(username) = &self.username {
            proxy = proxy.basic_auth(username, self.password.as_deref().unwrap_or(""));
        }

        Ok(proxy)
    }
}

fn split_host_port(s: &str) -> Result<(String, Option<u16>), String> {
    if let Some(rest) = s.strip_prefix('[') {
        // IPv6 literal: [host]:port or just [host]
        let close = rest
            .find(']')
            .ok_or_else(|| "unterminated '[' in IPv6 host".to_string())?;
        let host = format!("[{}]", &rest[..close]);
        let after = &rest[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(
                p.parse::<u16>()
                    .map_err(|_| format!("invalid port '{p}'"))?,
            ),
            None if after.is_empty() => None,
            None => return Err(format!("unexpected trailing data after host: '{after}'")),
        };
        return Ok((host, port));
    }

    match s.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| format!("invalid port '{port_str}'"))?;
            Ok((host.to_string(), Some(port)))
        }
        None => Ok((s.to_string(), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_host() {
        let p = ProxyConfig::parse("127.0.0.1:8080").unwrap();
        assert_eq!(p.scheme, ProxyScheme::Http);
        assert_eq!(p.host, "127.0.0.1");
        assert_eq!(p.port, Some(8080));
        assert!(p.username.is_none());
    }

    #[test]
    fn http_with_auth() {
        let p = ProxyConfig::parse("http://user:pass@proxy.example.com:3128").unwrap();
        assert_eq!(p.scheme, ProxyScheme::Http);
        assert_eq!(p.host, "proxy.example.com");
        assert_eq!(p.port, Some(3128));
        assert_eq!(p.username.as_deref(), Some("user"));
        assert_eq!(p.password.as_deref(), Some("pass"));
    }

    #[test]
    fn socks5_with_auth() {
        let p = ProxyConfig::parse("socks5://user:p@ss:w0rd@10.0.0.1:1080").unwrap();
        assert_eq!(p.scheme, ProxyScheme::Socks5);
        assert_eq!(p.host, "10.0.0.1");
        assert_eq!(p.port, Some(1080));
        assert_eq!(p.username.as_deref(), Some("user"));
        // password itself contains '@' and ':' — must survive intact
        assert_eq!(p.password.as_deref(), Some("p@ss:w0rd"));
    }

    #[test]
    fn no_port() {
        let p = ProxyConfig::parse("http://proxy.example.com").unwrap();
        assert_eq!(p.host, "proxy.example.com");
        assert_eq!(p.port, None);
    }

    #[test]
    fn ipv6_with_port() {
        let p = ProxyConfig::parse("[::1]:1080").unwrap();
        assert_eq!(p.host, "[::1]");
        assert_eq!(p.port, Some(1080));
    }

    #[test]
    fn rejects_empty() {
        assert!(ProxyConfig::parse("").is_err());
        assert!(ProxyConfig::parse("http://").is_err());
    }

    #[test]
    fn user_without_password() {
        let p = ProxyConfig::parse("http://user@proxy.example.com:8080").unwrap();
        assert_eq!(p.username.as_deref(), Some("user"));
        assert!(p.password.is_none());
    }
}
