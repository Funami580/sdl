use anyhow::Context;
use arc4::Arc4;
use base64::Engine;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header::HeaderName;
use url::Url;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};
use crate::download;

const KEYS_URL: &str = "https://raw.githubusercontent.com/KillerDogeEmpire/vidplay-keys/keys/keys.json";

pub struct Vidplay;

impl Extractor for Vidplay {
    const NAME: &'static str = "Vidplay/MyCloud";
    const NAMES: &'static [&'static str] = &["Vidplay", "MyCloud"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::Url
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(
            is_url_host_and_has_path(url, "vidplay.online", false, true)
                || is_url_host_and_has_path(url, "vidplay.site", false, true)
                || is_url_host_and_has_path(url, "mcloud.bz", false, true),
        )
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        let (url, user_agent) = match from {
            ExtractFrom::Url {
                url,
                user_agent,
                referer: _,
            } => (url, user_agent),
            ExtractFrom::Source(_) => anyhow::bail!("Vidplay/MyCloud: page source is not supported"),
        };
        let parsed_url = Url::parse(&url).with_context(|| "Vidplay/MyCloud: failed to parse url")?;
        let video_id = parsed_url
            .path_segments()
            .with_context(|| "Vidplay/MyCloud: failed to get path segments from parsed url")?
            .last()
            .unwrap();

        if video_id.is_empty() {
            anyhow::bail!("Vidplay/MyCloud: video id is empty");
        }

        let root_url = parsed_url
            .join("/")
            .with_context(|| "Vidplay/MyCloud: failed to create root url")?;
        let futoken_url = parsed_url
            .join("/futoken")
            .with_context(|| "Vidplay/MyCloud: failed to create futoken url")?;
        let futoken_text = download::get_page_text(futoken_url.as_str(), user_agent.as_deref(), Some(&url), None)
            .await
            .with_context(|| "Vidplay/MyCloud: failed to get futoken text")?;

        let keys = Self::get_keys().await?;
        let encoded_id = Self::encode_id(video_id, &keys.iter().map(String::as_str).collect::<Vec<_>>());
        let mediainfo_url = Self::get_mediainfo_url(&parsed_url, &futoken_text, &encoded_id)?;
        let mediainfo_extra_headers = [
            (
                reqwest::header::ACCEPT,
                "application/json, text/javascript, */*; q=0.01",
            ),
            (HeaderName::from_static("x-requested-with"), "XMLHttpRequest"),
        ];
        let mediainfo_json = download::get_page_json(
            &mediainfo_url,
            user_agent.as_deref(),
            Some(&url),
            Some(&mediainfo_extra_headers),
        )
        .await
        .with_context(|| "Vidplay/MyCloud: failed to get mediainfo json")?;

        let m3u8_url = mediainfo_json
            .as_object()
            .with_context(|| "Vidplay/MyCloud: mediainfo json is not object")?
            .get("result")
            .with_context(|| "Vidplay/MyCloud: mediainfo json does not have result key")?
            .as_object()
            .with_context(|| "Vidplay/MyCloud: result value in mediainfo json is not object")?
            .get("sources")
            .with_context(|| "Vidplay/MyCloud: mediainfo json does not have sources key")?
            .as_array()
            .with_context(|| "Vidplay/MyCloud: sources value in mediainfo json is not array")?
            .first()
            .with_context(|| "Vidplay/MyCloud: sources array in mediainfo json is empty")?
            .as_object()
            .with_context(|| "Vidplay/MyCloud: source value in mediainfo json is not object")?
            .get("file")
            .with_context(|| "Vidplay/MyCloud: source object in mediainfo json does not have file key")?
            .as_str()
            .with_context(|| "Vidplay/MyCloud: file value in mediainfo json is not string")?;

        Ok(ExtractedVideo {
            url: m3u8_url.to_owned(),
            referer: Some(root_url.into()),
        })
    }
}

impl Vidplay {
    async fn get_keys() -> Result<Vec<String>, anyhow::Error> {
        let json_response = download::get_page_json(KEYS_URL, None, None, None)
            .await
            .with_context(|| "Vidplay/MyCloud: failed to get keys")?;
        let key_array = json_response
            .as_array()
            .with_context(|| "Vidplay/MyCloud: keys not in array")?;
        let mut keys_vec = Vec::with_capacity(key_array.len());

        for key_value in key_array {
            let key = key_value
                .as_str()
                .with_context(|| "Vidplay/MyCloud: key is not string")?;
            keys_vec.push(key.to_owned());
        }

        Ok(keys_vec)
    }

    fn encode_id(video_id: &str, keys: &[&str]) -> String {
        let mut data = video_id.as_bytes().to_vec();

        for key in keys {
            let mut rc4 = Arc4::with_key(key.as_bytes());
            rc4.encrypt(&mut data);
        }

        let mut output = String::with_capacity(32);
        base64::prelude::BASE64_STANDARD.encode_string(data, &mut output);
        output.replace('/', "_")
    }

    fn get_mediainfo_url(parsed_url: &Url, futoken_text: &str, encoded_id: &str) -> Result<String, anyhow::Error> {
        static FUTOKEN_KEY_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"k='(\S+)'"#).unwrap());

        let futoken_key = FUTOKEN_KEY_REGEX
            .captures(futoken_text)
            .and_then(|captures| captures.get(1))
            .map(|futoken_key| futoken_key.as_str().to_string())
            .with_context(|| "Vidplay/MyCloud: failed to get futoken key")?;

        let mut data = Vec::with_capacity(encoded_id.len());

        for (index, id_code) in encoded_id.as_bytes().iter().enumerate() {
            data.push((futoken_key.as_bytes()[index % futoken_key.len()] + id_code).to_string());
        }

        let path = format!("/mediainfo/{futoken_key},{}", data.join(","));
        let mut mediainfo_url = parsed_url
            .join(&path)
            .with_context(|| "Vidplay/MyCloud: failed to create mediainfo url")?;
        mediainfo_url.set_query(parsed_url.query());

        Ok(mediainfo_url.into())
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::Vidplay;

    #[test]
    fn test_encode_id() {
        let video_id = "48YZZWELRY2X";
        let keys = ["oAPS7zX11zIzXFNi", "cWezD5NltrSMF7CG"];
        let encoded_id = Vidplay::encode_id(video_id, &keys);
        assert_eq!(&encoded_id, "+G6Ym2COlrtXDlUP");
    }

    #[test]
    fn test_get_mediainfo_url() {
        let futoken_text = r#"//
        // This is a mouse game for @enimax
        // Sponsored by the server resource.
        //
        (function () {window.requestInfo = function(v) {var k='VnBRsKNI5IqY-YchU70_TDDMLvoewQEQOOErJz7OlH-xS_wthFj9ONg932GXQXk=',a=[k];for(var i=0;i<v.length;i++)a.push(k.charCodeAt(i%k.length)+v.charCodeAt(i));return jQuery.ajax('mediainfo/'+a.join(',')+location.search,{dataType:'json'});};}());"#;

        let parsed_url =
            Url::parse("https://vidplay.online/e/48YZZWELRY2X?t=4xjQDvUhAFMNzA%3D%3D&autostart=true").unwrap();
        let video_id = parsed_url.path_segments().unwrap().last().unwrap();
        let keys = ["oAPS7zX11zIzXFNi", "cWezD5NltrSMF7CG"];
        let encoded_id = Vidplay::encode_id(video_id, &keys);
        let mediainfo_url = Vidplay::get_mediainfo_url(&parsed_url, futoken_text, &encoded_id).unwrap();
        assert_eq!(&mediainfo_url, "https://vidplay.online/mediainfo/VnBRsKNI5IqY-YchU70_TDDMLvoewQEQOOErJz7OlH-xS_wthFj9ONg932GXQXk=,129,181,120,171,224,125,145,152,161,187,229,177,113,197,184,184?t=4xjQDvUhAFMNzA%3D%3D&autostart=true");
    }
}
