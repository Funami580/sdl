use std::time::Duration;

use once_cell::sync::Lazy;
use regex::Regex;

use super::{DownloadSettings, InstantiatedDownloader};
use crate::downloaders::Downloader;

pub struct AniWorldSerienStream<'driver> {
    driver: &'driver mut thirtyfour::WebDriver,
    url: String,
}

impl<'driver> Downloader<'driver> for AniWorldSerienStream<'driver> {
    fn new(driver: &'driver mut thirtyfour::WebDriver, url: String) -> Self {
        Self { driver, url }
    }

    async fn supports_url(url: &str) -> bool {
        static SUPPORTS_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"(?i)^https?://(?:www\.)?(?:aniworld\.to/anime|s\.to/serie)/stream(?:/[^/\s]+){1,3}$"#)
                .unwrap()
        });
        SUPPORTS_URL_REGEX.is_match(url)
    }
}

impl InstantiatedDownloader for AniWorldSerienStream<'_> {
    async fn get_series_info(&self) -> Result<super::SeriesInfo, anyhow::Error> {
        todo!()
    }

    async fn download<F: FnMut() -> Duration>(
        &self,
        request: super::DownloadRequest,
        settings: &DownloadSettings<F>,
        sender: tokio::sync::mpsc::UnboundedSender<super::DownloadTask>,
    ) -> Result<(), anyhow::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::AniWorldSerienStream;
    use crate::downloaders::Downloader;

    #[tokio::test]
    async fn test_supports_url() {
        let is_supported = [
            "https://aniworld.to/anime/stream/detektiv-conan",
            "https://aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/staffel-1",
            "https://aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/filme",
            "https://aniworld.to/anime/stream/detektiv-conan/staffel-18/episode-2",
            "http://www.aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/filme",
            "https://s.to/serie/stream/detektiv-conan",
            "https://s.to/serie/stream/detektiv-conan/filme",
            "https://s.to/serie/stream/detektiv-conan/staffel-5",
            "https://s.to/serie/stream/detektiv-conan/staffel-1/episode-1",
        ];

        for url in is_supported {
            assert!(AniWorldSerienStream::supports_url(url).await);
        }
    }
}
