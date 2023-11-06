use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor};

pub struct Vidoza;

impl Extractor for Vidoza {
    async fn supports_url(url: &str) -> Option<bool> {
        Some(is_url_host_and_has_path(url, "vidoza.net", true, true))
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static VIDEO_URL_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"(?s)sourcesCode:\s\[\{\ssrc:\s"(.+)", type"#).unwrap());

        let source = from.get_source(None).await?;
        VIDEO_URL_REGEX
            .captures(&source)
            .and_then(|captures| captures.get(1))
            .map(|video_url| ExtractedVideo {
                url: video_url.as_str().to_string(),
                referer: None,
            })
            .with_context(|| "Vidoza: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Vidoza;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_vidoza() {
        let url = "https://vidoza.net/embed-something.html";
        assert!(Vidoza::supports_url(url).await.unwrap_or(false));

        let source = r#"window.pData = {
            isEmbed: '1',
            preload: 'auto',
            width: "1280",
            height: "720",
            poster: "https://str27.vidoza.net/i/01/07196/fgjnd9kwws06.jpg?v=1697939546",
            volume: 1,
            sourcesCode: [{ src: "https://str27.vidoza.net/nvl4cwn3difeieno3w5qpdfjmx3swwnezlnhwfbr55tzrudvhhyo7ndvgxra/v.mp4", type: "video/mp4", label:"SD", res:"720"}],
            topBarButtons: {feedback: {icon: 'fa-commenting-o',title: 'Feedback'}},
            x2time: 85,
            vtime: 170,
            user_id: '5db7ddeb4edcd758d0b4bb101ba18899',
            playIdMd5: '95de3338335a8e5ee34b9cb4887b1dc7',
            user_ip: '185.249.168.15',
            file_refer: '',
            file_code: 'fgjnd9kwws06',
            file_id: '35982466',
            file_ophash: '35982466-185-249-1697939546-d68ccac820cc6b7f88c625d1e2203f96',
            server_id: '1027',
            disk_id: '407',
            host_name: 'gl-ams-str-27',
            host_dc: '',
            host_group: '0STORAGE',
            host_hls: '0',
            site_url: 'https://vidoza.net',"#;
        let expected = "https://str27.vidoza.net/nvl4cwn3difeieno3w5qpdfjmx3swwnezlnhwfbr55tzrudvhhyo7ndvgxra/v.mp4";

        let extracted = Vidoza::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
