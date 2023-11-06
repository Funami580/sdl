use super::{ExtractFrom, ExtractedVideo, Extractor};

pub struct Dummy;

impl Extractor for Dummy {
    async fn supports_url(_: &str) -> Option<bool> {
        None
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        match from {
            ExtractFrom::Url {
                url,
                user_agent: _,
                referer,
            } => Ok(ExtractedVideo { url, referer }),
            ExtractFrom::Source(_) => anyhow::bail!("Dummy: page source is not supported"),
        }
    }
}
