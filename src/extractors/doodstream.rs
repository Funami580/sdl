use std::time::SystemTime;

use anyhow::Context;
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use regex::Regex;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};
use crate::download::{self, get_page_text};

pub struct Doodstream;

impl Extractor for Doodstream {
    const DISPLAY_NAME: &'static str = "Doodstream";
    const NAMES: &'static [&'static str] = &["Doodstream"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::Url
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(
            is_url_host_and_has_path(url, "dood.li", true, true)
                || is_url_host_and_has_path(url, "dood.la", true, true)
                || is_url_host_and_has_path(url, "ds2video.com", true, true)
                || is_url_host_and_has_path(url, "ds2play.com", true, true)
                || is_url_host_and_has_path(url, "dood.yt", true, true)
                || is_url_host_and_has_path(url, "dood.ws", true, true)
                || is_url_host_and_has_path(url, "dood.so", true, true)
                || is_url_host_and_has_path(url, "dood.to", true, true)
                || is_url_host_and_has_path(url, "dood.pm", true, true)
                || is_url_host_and_has_path(url, "dood.watch", true, true)
                || is_url_host_and_has_path(url, "dood.sh", true, true)
                || is_url_host_and_has_path(url, "dood.cx", true, true)
                || is_url_host_and_has_path(url, "dood.wf", true, true)
                || is_url_host_and_has_path(url, "dood.re", true, true)
                || is_url_host_and_has_path(url, "dood.one", true, true)
                || is_url_host_and_has_path(url, "dood.tech", true, true)
                || is_url_host_and_has_path(url, "dood.work", true, true)
                || is_url_host_and_has_path(url, "doods.pro", true, true)
                || is_url_host_and_has_path(url, "dooood.com", true, true)
                || is_url_host_and_has_path(url, "doodstream.com", true, true)
                || is_url_host_and_has_path(url, "doodstream.co", true, true)
                || is_url_host_and_has_path(url, "d000d.com", true, true)
                || is_url_host_and_has_path(url, "d0000d.com", true, true)
                || is_url_host_and_has_path(url, "doodapi.com", true, true)
                || is_url_host_and_has_path(url, "d0o0d.com", true, true)
                || is_url_host_and_has_path(url, "do0od.com", true, true)
                || is_url_host_and_has_path(url, "dooodster.com", true, true)
                || is_url_host_and_has_path(url, "vidply.com", true, true)
                || is_url_host_and_has_path(url, "do7go.com", true, true)
                || is_url_host_and_has_path(url, "all3do.com", true, true)
                || is_url_host_and_has_path(url, "doply.net", true, true),
        )
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static FETCH_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"(?s)\$\.get\(\s*['"](/pass_md5/[\w-]+/([\w-]+))['"]\s*,\s*function\(\s*data\s*\)"#).unwrap()
        });
        const RANDOM_STRING_CHARS: &[u8] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".as_bytes();

        // Extract current url info etc.
        let (current_url, user_agent, current_referer) = match from {
            ExtractFrom::Url {
                url,
                user_agent,
                referer,
            } => (url, user_agent, referer),
            ExtractFrom::Source(_) => anyhow::bail!("Doodstream: from source is unsupported"),
        };
        let current_url = url::Url::parse(&current_url).context("Doodstream: failed to retrieve sources")?;

        // Resolve url and get text response
        let response = download::get_response(
            None,
            current_url,
            user_agent.as_deref(),
            current_referer.as_deref(),
            None,
        )
        .await
        .context("Doodstream: failed to retrieve sources")?
        .response();

        let resolved_url = response.url().clone();
        let source = response
            .text()
            .await
            .context("Doodstream: failed to retrieve sources")?;

        // Extract video link
        let (relative_fetch_url, token) = FETCH_REGEX
            .captures(&source)
            .and_then(|captures| captures.get(1).zip(captures.get(2)))
            .map(|(m1, m2)| (m1.as_str().to_string(), m2.as_str().to_string()))
            .context("Doodstream: failed to retrieve sources")?;

        let video_base_url = {
            let fetch_url = resolved_url
                .join(&relative_fetch_url)
                .context("Doodstream: failed to retrieve sources")?;
            let fetch_referer = resolved_url.as_str().to_string();
            get_page_text(fetch_url, user_agent.as_deref(), Some(&fetch_referer), None)
                .await
                .context("Doodstream: failed to retrieve sources")?
        };
        let random_string = {
            let mut rng = rand::thread_rng();
            String::from_utf8(
                std::iter::repeat_with(|| *RANDOM_STRING_CHARS.choose(&mut rng).unwrap())
                    .take(10)
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let unix_time_millis = {
            let start = SystemTime::now();
            let since_the_epoch = start
                .duration_since(std::time::UNIX_EPOCH)
                .context("Doodstream: failed to retrieve sources: system time before Unix epoch")?;
            since_the_epoch.as_millis()
        };

        let video_url = format!("{video_base_url}{random_string}?token={token}&expiry={unix_time_millis}");
        let video_url_referer = resolved_url
            .join("/")
            .context("Doodstream: failed to retrieve sources")?;

        Ok(ExtractedVideo {
            url: video_url,
            referer: Some(video_url_referer.as_str().to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Doodstream;
    use crate::extractors::Extractor;

    #[tokio::test]
    async fn test_doodstream() {
        let url = "https://dood.li/e/s23ywsyo2fbm";
        assert!(Doodstream::supports_url(url).await.unwrap_or(false));
    }
}
