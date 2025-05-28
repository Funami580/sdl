use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};

pub struct Streamtape;

impl Extractor for Streamtape {
    const DISPLAY_NAME: &'static str = "Streamtape";
    const NAMES: &'static [&'static str] = &["Streamtape"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::all()
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(
            is_url_host_and_has_path(url, "streamtape.com", true, true)
                || is_url_host_and_has_path(url, "shavetape.cash", true, true)
                || is_url_host_and_has_path(url, "streamtape.xyz", true, true)
                || is_url_host_and_has_path(url, "streamtape.net", true, true),
        )
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static ROBOT_LINK_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"<div\s*[^>]*?id="robotlink"[^>]*?>[^<]*?(/get_video[^<]+?)</div>"#).unwrap());
        static TOKEN_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"&token=([^&?\s'"]+)"#).unwrap());

        let source = from.get_source(None).await?;
        ROBOT_LINK_REGEX
            .captures(&source)
            .and_then(|captures| captures.get(1).map(|group| group.as_str()))
            .and_then(|robot_url| {
                let token = TOKEN_REGEX
                    .captures_iter(&source)
                    .last()
                    .and_then(|captures| captures.get(1).map(|group| group.as_str()));
                token.map(|token| (robot_url, token))
            })
            .and_then(|(robot_url, token)| {
                let mut streamtape_url = Url::parse(&format!("https://streamtape.com{}", robot_url)).ok()?;
                let new_query = form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(streamtape_url.query_pairs().filter(|(key, _)| key != "token"))
                    .append_pair("token", token)
                    .append_pair("stream", "1")
                    .finish();
                streamtape_url.set_query(Some(&new_query));

                let extracted_video = ExtractedVideo {
                    url: streamtape_url.as_str().to_string(),
                    referer: None,
                };

                Some(extracted_video)
            })
            .context("Streamtape: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Streamtape;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_streamtape() {
        let url = "https://streamtape.com/e/jv430mJ2bOszzOB";
        assert!(Streamtape::supports_url(url).await.unwrap_or(false));

        let source = r##"<div class="play-overlay"></div>			<video crossorigin="anonymous" id="mainvideo" width="100%" height="100%"  poster="https://thumb.tapecontent.net/thumb/jv430mJ2bOszzOB/7m9Kp3YGjoIA7aX.jpg" playsinline preload="metadata" >
        </video><script>if(navigator.userAgent.indexOf("TV") == -1){ window.player=new Plyr("video");}else{document.getElementById("mainvideo").setAttribute("controls", "controls");window.procsubs();}</script>
                                
                            </div>
                <div id="ideoolink" style="display:none;">/streamtape.com/get_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJcde</div>
        <span id="botlink" style="display:none;">/streamtape.com/get_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMxyza</span>
        <div id="robotlink" style="display:none;">/streamtape.com/get_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJcde</div>
        <script>document.getElementById('ideoolink').innerHTML = "/streamtape.com/get" + ''+ ('xcdb_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJjx6').substring(1).substring(2);
        document.getElementById('ideoolink').innerHTML = "//streamtape.com/ge" + ''+ ('xnftb_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJjx6').substring(3).substring(1);
        document.getElementById('botlink').innerHTML = '//streamtape.com/ge'+ ('xyzat_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJjx6').substring(4);
        document.getElementById('robotlink').innerHTML = '//streamtape.com/ge'+ ('xcdt_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJjx6').substring(2).substring(1);
        </script>
        <script>$("#loading").remove();$("body").removeClass('loader')</script>"##;
        let expected = "https://streamtape.com/get_video?id=jv430mJ2bOszzOB&expires=1698017179&ip=F0uRKRSNFI9XKxR&token=TIdWaxtMJjx6&stream=1";

        let extracted = Streamtape::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
