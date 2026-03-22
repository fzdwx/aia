use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const DEFAULT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WeixinClientConfig {
    pub(crate) base_url: String,
    pub(crate) bot_token: Option<String>,
    pub(crate) route_tag: Option<String>,
    pub(crate) cdn_base_url: String,
    pub(crate) api_timeout: Duration,
    pub(crate) config_timeout: Duration,
    pub(crate) long_poll_timeout: Duration,
    pub(crate) qr_poll_timeout: Duration,
}

impl WeixinClientConfig {
    pub fn new(base_url: impl Into<String>, bot_token: Option<&str>) -> Self {
        Self {
            base_url: ensure_trailing_slash(base_url.into()),
            bot_token: bot_token.map(str::to_owned),
            route_tag: None,
            cdn_base_url: ensure_trailing_slash(DEFAULT_CDN_BASE_URL),
            api_timeout: Duration::from_millis(15_000),
            config_timeout: Duration::from_millis(10_000),
            long_poll_timeout: Duration::from_millis(35_000),
            qr_poll_timeout: Duration::from_millis(35_000),
        }
    }

    pub fn default_base_url() -> &'static str {
        DEFAULT_BASE_URL
    }

    pub fn with_route_tag(mut self, route_tag: Option<&str>) -> Self {
        self.route_tag = route_tag.map(str::to_owned).filter(|value| !value.trim().is_empty());
        self
    }

    pub fn with_cdn_base_url(mut self, cdn_base_url: impl Into<String>) -> Self {
        self.cdn_base_url = ensure_trailing_slash(cdn_base_url.into());
        self
    }

    pub fn with_api_timeout(mut self, timeout: Duration) -> Self {
        self.api_timeout = timeout;
        self
    }

    pub fn with_config_timeout(mut self, timeout: Duration) -> Self {
        self.config_timeout = timeout;
        self
    }

    pub fn with_long_poll_timeout(mut self, timeout: Duration) -> Self {
        self.long_poll_timeout = timeout;
        self
    }

    pub fn with_qr_poll_timeout(mut self, timeout: Duration) -> Self {
        self.qr_poll_timeout = timeout;
        self
    }
}

fn ensure_trailing_slash(value: impl AsRef<str>) -> String {
    let trimmed = value.as_ref().trim();
    let base = if trimmed.is_empty() { DEFAULT_BASE_URL } else { trimmed };
    if base.ends_with('/') { base.to_owned() } else { format!("{base}/") }
}
