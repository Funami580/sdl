use super::InstantiatedDownloader;
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
        todo!()
    }
}

impl InstantiatedDownloader for AniWorldSerienStream<'_> {
    async fn get_series_info(&self) -> Result<super::SeriesInfo, anyhow::Error> {
        todo!()
    }

    async fn download(
        &self,
        request: super::DownloadRequest,
        sender: tokio::sync::mpsc::UnboundedSender<super::DownloadTask>,
    ) -> Result<(), anyhow::Error> {
        todo!()
    }
}
