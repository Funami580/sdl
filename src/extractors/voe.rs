use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;

use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};

pub struct Voe;

impl Extractor for Voe {
    const NAME: &'static str = "Voe";
    const NAMES: &'static [&'static str] = &["Voe"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::all()
    }

    async fn supports_url(_url: &str) -> Option<bool> {
        None
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"'hls': '([^']+)'"#).unwrap());

        let source = from.get_source(None).await?;
        VIDEO_URL_REGEX
            .captures(&source)
            .and_then(|captures| captures.get(1))
            .map(|video_url| ExtractedVideo {
                url: video_url.as_str().to_string(),
                referer: None,
            })
            .with_context(|| "Voe: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Voe;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_voe() {
        let source = "var sources = {↵
            'hls': 'https://delivery-node-p529oubjmokvzzdm.voe-network.net/engine/hls2-c/01/09512/ytd65pzpecoo_,n,.urlset/master.m3u8?t=1jRM2PpnYUY4QwQMhav3le-633OJQgg8HaOovs0Tf18&s=1697938932&e=14400&f=47560621&node=delivery-node-ynug3prrg0f4gget.voe-network.net&i=185.249&sp=2500&asn=12329',↵
            'video_height': 720,↵
                    };↵";
        let expected = "https://delivery-node-p529oubjmokvzzdm.voe-network.net/engine/hls2-c/01/09512/ytd65pzpecoo_,n,.urlset/master.m3u8?t=1jRM2PpnYUY4QwQMhav3le-633OJQgg8HaOovs0Tf18&s=1697938932&e=14400&f=47560621&node=delivery-node-ynug3prrg0f4gget.voe-network.net&i=185.249&sp=2500&asn=12329";

        let extracted = Voe::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
