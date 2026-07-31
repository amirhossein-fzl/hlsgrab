use std::time::Duration;

use reqwest::header::{COOKIE, HeaderMap, HeaderName, HeaderValue};

use crate::downloader::error::DownloaderError;
use crate::downloader::proxy::ProxyConfig;

pub struct DownloaderClientBuilder {
    headers: HeaderMap,
    timeout: Option<Duration>,
    proxy: Option<ProxyConfig>,
}

impl DownloaderClientBuilder {
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            timeout: None,
            proxy: None,        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Result<Self, DownloaderError> {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| DownloaderError::InvalidHeaderName(name.to_string()))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| DownloaderError::InvalidHeaderValue(name.to_string()))?;
        self.headers.insert(header_name, header_value);
        Ok(self)
    }

    pub fn headers<I, K, V>(mut self, headers: I) -> Result<Self, DownloaderError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (k, v) in headers {
            self = self.header(k.as_ref(), v.as_ref())?;
        }
        Ok(self)
    }

    pub fn cookie_str(mut self, cookie: &str) -> Result<Self, DownloaderError> {
        let value = HeaderValue::from_str(cookie)
            .map_err(|_| DownloaderError::InvalidHeaderValue(COOKIE.to_string()))?;
        self.headers.insert(COOKIE, value);
        Ok(self)
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn proxy(mut self, proxy: &str) -> Result<Self, DownloaderError> {
        self.proxy = Some(ProxyConfig::parse(proxy)?);
        Ok(self)
    }

    pub fn build(self) -> Result<DownloaderClient, DownloaderError> {
        let mut builder = reqwest::Client::builder()
            .default_headers(self.headers);

        if let Some(t) = self.timeout {
            builder = builder.connect_timeout(t);
        }
        if let Some(proxy_cfg) = self.proxy {
            let proxy = proxy_cfg.into_reqwest_proxy()?;
            builder = builder.proxy(proxy);
        }

        let client = builder.build().map_err(DownloaderError::ClientBuild)?;

        Ok(DownloaderClient {
            client,        })
    }
}

impl Default for DownloaderClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct DownloaderClient {
    pub(crate) client: reqwest::Client,
}

impl DownloaderClient {
    pub fn builder() -> DownloaderClientBuilder {
        DownloaderClientBuilder::new()
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}
