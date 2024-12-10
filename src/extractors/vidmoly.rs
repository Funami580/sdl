use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};

pub struct Vidmoly;

impl Extractor for Vidmoly {
    const DISPLAY_NAME: &'static str = "Vidmoly";
    const NAMES: &'static [&'static str] = &["Vidmoly"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::all()
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(is_url_host_and_has_path(url, "vidmoly.to", true, true))
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?s)file:\s*"([^"]+\.m3u8[^"]*)""#).unwrap());

        let source = from.get_source(None).await?;
        VIDEO_URL_REGEX
            .captures(&source)
            .and_then(|captures| captures.get(1))
            .map(|video_url| ExtractedVideo {
                url: video_url.as_str().to_string(),
                referer: Some("https://vidmoly.to/".to_string()),
            })
            .context("Vidmoly: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Vidmoly;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_vidmoly() {
        let url = "https://vidmoly.to/embed-z4knfpsh2q3o.html";
        assert!(Vidmoly::supports_url(url).await.unwrap_or(false));

        let source = r#"  var player = jwplayer("vplayer");
  const playerInstance = 
  player.setup({
    sources: [{file:"https://box-1031-f.vmeas.cloud/hls/xqx2pso7grokjiqbtfvchm2axjkaannuk4e6hwump,byztove2jkaai2yqgpa,ikztove2jkavsbrjbqq,.urlset/master.m3u8"}],
    image: "https://box-1031-f.vmeas.cloud/i/01/01384/z4knfpsh2q3o.jpg",
    bitrate: "2160000",
    label: "720p HD",
    width: "100%", 
    height: "100%",
    cast: {},
    stretching: "",
    duration: "1425",
    //aspectratio: "16:9",
    preload: 'none',
    bufferPercent: '5090',
    defaultBandwidthEstimate: "250000",
    androidhls: "true",
    hlshtml: "true",
    primary: "html5",
    playbackRateControls: "false",
    playbackRates: [0.25, 0.5, 1, 1.5, 2.0],
    startparam: "start",
    "skin": {
    "name": "alaska"
    },
    advertising: molyast21
    ,tracks: [{file: "/dl?op=get_slides&length=1425&url=https://box-1031-f.vmeas.cloud/i/01/01384/z4knfpsh2q3o0000.jpg", kind: "thumbnails"}]
    ,captions: {color: '#FFFFFF', fontSize: 16, fontFamily:"Verdana", backgroundOpacity: 0, edgeStyle: 'raised', fontOpacity: 90},'qualityLabels':{"2078":"HD","799":"SD"},related: {file:"", onclick:"link"}
  });"#;
        let expected = "https://box-1031-f.vmeas.cloud/hls/xqx2pso7grokjiqbtfvchm2axjkaannuk4e6hwump,byztove2jkaai2yqgpa,ikztove2jkavsbrjbqq,.urlset/master.m3u8";

        let extracted = Vidmoly::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
