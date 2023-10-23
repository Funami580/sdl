use std::time::Duration;

use super::{DownloadSettings, InstantiatedDownloader};
use crate::downloaders::Downloader;

pub struct Aniwave<'driver> {
    driver: &'driver mut thirtyfour::WebDriver,
    url: String,
}

impl<'driver> Downloader<'driver> for Aniwave<'driver> {
    fn new(driver: &'driver mut thirtyfour::WebDriver, url: String) -> Self {
        Self { driver, url }
    }

    async fn supports_url(url: &str) -> bool {
        todo!()
    }
}

impl InstantiatedDownloader for Aniwave<'_> {
    async fn get_series_info(&self) -> Result<super::SeriesInfo, anyhow::Error> {
        todo!()
    }

    async fn download<F: FnMut() -> Duration>(
        &self,
        request: super::DownloadRequest,
        settings: DownloadSettings<F>,
        sender: tokio::sync::mpsc::UnboundedSender<super::DownloadTask>,
    ) -> Result<(), anyhow::Error> {
        todo!()
    }
}
