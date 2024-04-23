use anyhow::Context;
use base64::Engine;
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
        let source = from.get_source(None).await?;
        Self::extract1(&source).or_else(|_| Self::extract2(&source))
    }
}

impl Voe {
    fn extract1(source: &str) -> Result<ExtractedVideo, anyhow::Error> {
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"'hls': '([^']+)'"#).unwrap());

        VIDEO_URL_REGEX
            .captures(source)
            .and_then(|captures| captures.get(1))
            .map(|video_url| {
                let url = base64::prelude::BASE64_STANDARD
                    .decode(video_url.as_str())
                    .ok()
                    .and_then(|decoded_url| String::from_utf8(decoded_url).ok())
                    .unwrap_or(video_url.as_str().to_string());

                ExtractedVideo { url, referer: None }
            })
            .with_context(|| "Voe: failed to retrieve sources")
    }

    fn extract2(source: &str) -> Result<ExtractedVideo, anyhow::Error> {
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r#"let \w+ = '((?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{4}|[A-Za-z0-9+/]{3}=|[A-Za-z0-9+/]{2}={2}))';"#,
            )
            .unwrap()
        });

        VIDEO_URL_REGEX
            .captures(source)
            .and_then(|captures| captures.get(1))
            .and_then(|capture| {
                base64::prelude::BASE64_STANDARD
                    .decode(capture.as_str())
                    .ok()
                    .map(|mut reversed_json| {
                        reversed_json.reverse();
                        reversed_json
                    })
            })
            .and_then(|json_text| serde_json::from_slice::<serde_json::value::Value>(&json_text).ok())
            .and_then(|json| {
                json.as_object()
                    .and_then(|json_object| json_object.get("file"))
                    .and_then(|json_file| json_file.as_str().map(|s| s.to_string()))
            })
            .map(|video_url| ExtractedVideo {
                url: video_url,
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
    async fn test_voe1() {
        let source = "var sources = {
            'hls': 'https://delivery-node-oxccnn9nkkcxh4ir.voe-network.net/engine/hls2-c/01/10111/n6odadstvbey_,n,.urlset/master.m3u8?t=X73hbD6gRBj2AD2BPe7eamyuaXyBrqJmCPXMLvapNsA&s=1710100837&e=14400&f=50556351&node=delivery-node-sq0tgp6zzeel6axe.voe-network.net&i=217.232&sp=2500&asn=3320',
            'video_height': 720,
                    };";
        let expected = "https://delivery-node-oxccnn9nkkcxh4ir.voe-network.net/engine/hls2-c/01/10111/n6odadstvbey_,n,.urlset/master.m3u8?t=X73hbD6gRBj2AD2BPe7eamyuaXyBrqJmCPXMLvapNsA&s=1710100837&e=14400&f=50556351&node=delivery-node-sq0tgp6zzeel6axe.voe-network.net&i=217.232&sp=2500&asn=3320";

        let extracted = Voe::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }

    #[tokio::test]
    async fn test_voe2() {
        let source = "<script>
        let f62aad852c654bf8c9737da67c45630c7dec5019 = 'fTA6Imtjb2xiZGFfbm9pdGF6aXRlbm9tIiwwOiJlbGlmb3JwIiwwOiJtdWltZXJwIiwwOiJwZm8iLCIudGkgZGFvbCBvdCBnbml5cnQgZWxpaHcgcm9ycmUgbmEgc2F3IGVyZWh0IHJvICxkZWRhb2wgZWIgdCduZGx1b2MgKW9lZGl2IGduaW1hZXJ0cyByb2YgZGVzdSBlbGlmIGEoIHRzZWZpbmFtIFNMSCBlaFQgLmtyb3d0ZW4gZWh0IGh0aXcgbWVsYm9ycCBhIHMnZXJlaHQgdHViICx5cnJvUyI6Im5vaXRhbmFscHhlX3JvcnJlX2tyb3d0ZW4iLCJuaWFnYSByZXRhbCB5clQgLSByb3JyRSBrcm93dGVOIjoicm9ycmVfa3Jvd3RlbiIsZXNsYWY6ImRhb2xfb3R1YSIsMDAwMDAwMDU6ImV6aXNfcmVmZnViIiwwODE6Imh0Z25lbF9yZWZmdWIiLCIiOiJrY2FibGxhZiIsIjAyMzM9bnNhJjAwNTI9cHMmMjMyLjcxMj1pJnRlbi5rcm93dGVuLWVvdi4waTZtcWYxYmx1eHB4NzVxLWVkb24teXJldmlsZWQ9ZWRvbiYwOTk5ODg3PWYmMDA0NDE9ZSYyMzkyMTI5MDcxPXMma2NoeVJmRGh0b1BjQU9XV1BXRW84MUhYcXNMZkpHU1djSkpDc0tRdVRGRj10Pzh1M20ucmV0c2FtL1x0ZXNscnUuLG4sX3ljd20wdmJjamhzcC9cNzc1MTAvXDEwL1wyc2xoL1xlbmlnbmUvXHRlbi5rcm93dGVuLWVvdi4waTZtcWYxYmx1eHB4NzVxLWVkb24teXJldmlsZWQvXC9cOnNwdHRoIjoiZWxpZiIsMToiZGVibWUiLCJ5Y3dtMHZiY2poc3AiOiJlZG9jIiwxMzQ6ImVkb24iLCIua2NlaGMgbGF1bmFtIGEgcm9mIHRzaWwgYSBuaSBzbGlhdGVkIGVjaXZlZC9ccmVzd29yYiBydW95IGRlZGRhIGV2YWggZVcgLmRldHJvcHB1cyB0b24gc2kgZWNpdmVkL1xyZXN3b3JiIHJ1b3kgZWtpbCBza29vbCB0SSI6Im5vaXRhbmFscHhlX2RldHJvcHB1c190b25fcmVzd29yYiIsImRldHJvcHB1cyB0b24gZWNpdmVEL1xyZXN3b3JCIjoiZGV0cm9wcHVzX3Rvbl9yZXN3b3JiIns=';
        </script>";
        let expected = "https://delivery-node-q57xpxulb1fqm6i0.voe-network.net/engine/hls2/01/01577/pshjcbv0mwcy_,n,.urlset/master.m3u8?t=FFTuQKsCJJcWSGJfLsqXH18oEWPWWOAcPothDfRyhck&s=1709212932&e=14400&f=7889990&node=delivery-node-q57xpxulb1fqm6i0.voe-network.net&i=217.232&sp=2500&asn=3320";

        let extracted = Voe::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }

    #[tokio::test]
    async fn test_voe3() {
        let source = "var sources = {
            'hls': 'aHR0cHM6Ly9kZWxpdmVyeS1ub2RlLW9jZGQ1b2l5bGI4ZmZpb3oudm9lLW5ldHdvcmsubmV0L2VuZ2luZS9obHMyLWMvMDEvMTA3NjMvYWd6bXdhYXRhNW96XyxuLGwsLnVybHNldC9tYXN0ZXIubTN1OD90PUVQZUVlRjQwYkx1eEZyeVF5VEYyWFFvZE1jcnZWenF1RnYyUFR2LXF3MkEmcz0xNzEzODk2OTY0JmU9MTQ0MDAmZj01MzgxNzA3NiZub2RlPWRlbGl2ZXJ5LW5vZGUtZHpsN2p3bHdpY3l6eTV6bS52b2UtbmV0d29yay5uZXQmaT0yMTcuODcmc3A9MjUwMCZhc249MzMyMA==',
            'video_height': 720,
                    };";
        let expected = "https://delivery-node-ocdd5oiylb8ffioz.voe-network.net/engine/hls2-c/01/10763/agzmwaata5oz_,n,l,.urlset/master.m3u8?t=EPeEeF40bLuxFryQyTF2XQodMcrvVzquFv2PTv-qw2A&s=1713896964&e=14400&f=53817076&node=delivery-node-dzl7jwlwicyzy5zm.voe-network.net&i=217.87&sp=2500&asn=3320";

        let extracted = Voe::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
