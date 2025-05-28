use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header::HeaderValue;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};
use crate::download;
use crate::extractors::utils::decode_packed_codes;

pub struct LoadX;

impl Extractor for LoadX {
    const DISPLAY_NAME: &'static str = "LoadX";
    const NAMES: &'static [&'static str] = &["LoadX"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::all()
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(is_url_host_and_has_path(url, "loadx.ws", true, false))
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static EVAL_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"(?s)(eval\(function\(p,a,c,k,e,d\).+?)</script>"#).unwrap());
        static ID_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"FirePlayer\(\s*"([^"]+)""#).unwrap());

        // Get source
        let user_agent = match &from {
            ExtractFrom::Url {
                url: _,
                user_agent,
                referer: _,
            } => user_agent.clone(),
            ExtractFrom::Source(_) => None,
        };
        let source = from.get_source(None).await?;

        // Get id
        let mut id = None;
        for eval in EVAL_REGEX.captures_iter(&source) {
            let Some(eval_content) = eval.get(1).map(|group| group.as_str().trim()) else {
                continue;
            };

            let Some(unpacked_script) = decode_packed_codes(eval_content) else {
                continue;
            };

            if let Some(found_id) = ID_REGEX
                .captures(&unpacked_script)
                .and_then(|captures| captures.get(1))
                .map(|id| id.as_str().to_string())
            {
                id = Some(found_id);
                break;
            }
        }
        let Some(id) = id else {
            anyhow::bail!("LoadX: failed to retrieve sources")
        };

        // Get link to m3u8 playlist
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            HeaderValue::from_str(user_agent.as_deref().unwrap_or(download::DEFAULT_USER_AGENT))
                .context("LoadX: failed to set user agent")?,
        );
        headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
        headers.insert("X-Requested-With", HeaderValue::from_static("XMLHttpRequest"));
        headers.insert(
            "Content-Type",
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        let response = download::DEFAULT_RETRY_CLIENT_NO_REDIRECT
            .post(format!("https://loadx.ws/player/index.php?data={id}&do=getVideo"))
            .headers(headers)
            .body(format!("hash={id}&r="))
            .send()
            .await
            .context("LoadX: failed to retrieve sources")?
            .text()
            .await
            .context("LoadX: failed to retrieve sources")?;
        let json: serde_json::Value = serde_json::from_str(&response)
            .ok()
            .context("LoadX: failed to retrieve sources")?;
        let m3u8_url = json.get("videoSource").and_then(|v| v.as_str().map(|s| s.to_string()));

        if let Some(m3u8_url) = m3u8_url {
            return Ok(ExtractedVideo {
                url: m3u8_url,
                referer: None,
            });
        }

        anyhow::bail!("LoadX: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::LoadX;
    use crate::extractors::Extractor;

    #[tokio::test]
    async fn test_loadx() {
        let url = "https://loadx.ws/video/9436c43d396c2eb01c5d1e2f0b1e510d";
        assert!(LoadX::supports_url(url).await.unwrap_or(false));
    }
}
