use std::borrow::Cow;
use std::ops::Deref;
use std::path::PathBuf;

use chrono::Local;
use clap::Parser;
use cli::{Args, Extractor};
use download::{DownloadManager, Downloader, InternalDownloadTask};
use downloaders::{DownloadRequest, InstantiatedDownloader};
use extractors::{extract_video_url, extract_video_url_with_extractor};
use ffmpeg::Ffmpeg;
use logger::log_wrapper::{LogWrapper, SetLogWrapper};

pub(crate) mod chrome;
pub(crate) mod cli;
pub(crate) mod dirs;
pub(crate) mod download;
pub(crate) mod downloaders;
pub(crate) mod extractors;
pub(crate) mod ffmpeg;
pub(crate) mod logger;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Parse arguments
    let args = cli::Args::parse();
    let debug = args.debug;
    let extractor = args.extractor.as_ref();

    // Set up logger
    let logger = logger::default_logger(debug);
    let mut log_wrapper = LogWrapper::new(None, logger).try_init().unwrap();

    // Create data dir
    let data_dir = match dirs::get_data_dir().await {
        Ok(data_dir) => data_dir,
        Err(err) => {
            log::error!("Failed to create data directory: {:#}", err);
            std::process::exit(1);
        }
    };

    // Get save directory
    let save_directory = match dirs::get_save_directory(None) {
        Ok(dir) => dir,
        Err(err) => {
            log::error!("Failed to get save directory: {:#}", err);
            std::process::exit(1);
        }
    };

    // Set up FFmpeg, and ChromeDriver if needed
    let asset_downloader = Downloader::new(&mut log_wrapper, debug, None, None);
    let ffmpeg = Ffmpeg::new(data_dir.clone());

    let (mut chrome, ffmpeg_install_result) = if extractor.is_none() {
        let (chrome, ffmpeg_install_result) = tokio::join!(
            chrome::ChromeDriver::get(&data_dir, &asset_downloader, !debug),
            ffmpeg.auto_download(&asset_downloader),
        );

        let chrome = match chrome {
            Ok(chrome) => chrome,
            Err(err) => {
                log::error!("Failed to create ChromeDriver: {:#}", err);
                std::process::exit(1);
            }
        };

        (Some(chrome), ffmpeg_install_result)
    } else {
        (None, ffmpeg.auto_download(&asset_downloader).await)
    };

    // Do much of the bulk work
    let should_error_quit = do_after_chrome_driver(
        ffmpeg_install_result,
        asset_downloader,
        chrome.as_mut(),
        log_wrapper,
        save_directory,
        args,
    )
    .await;

    // Quit ChromeDriver
    if let Some(chrome) = chrome {
        if let Err(err) = chrome.quit().await {
            log::warn!("Failed to quit ChromeDriver: {}", err);
        }
    }

    if should_error_quit {
        std::process::exit(1);
    }
}

async fn do_after_chrome_driver(
    ffmpeg_install_result: Result<PathBuf, anyhow::Error>,
    asset_downloader: Downloader,
    chrome: Option<&mut thirtyfour::WebDriver>,
    mut log_wrapper: SetLogWrapper,
    save_directory: PathBuf,
    args: Args,
) -> bool {
    let debug = args.debug;
    let extractor = args.extractor.as_ref();
    let url = args.url.deref();
    let max_concurrent = args.concurrent_downloads;

    let ffmpeg_path = match ffmpeg_install_result {
        Ok(path) => path,
        Err(err) => {
            log::error!("Failed to get path to FFmpeg: {:#}", err);
            return true;
        }
    };

    asset_downloader.clear();

    // Download episodes
    let user_agent = match &chrome {
        Some(driver) => chrome::get_user_agent(driver).await,
        None => None,
    };
    let episodes_downloader = Downloader::new(&mut log_wrapper, debug, Some(ffmpeg_path), user_agent);

    if let Some(extractor) = extractor {
        let extractor_result = if let Extractor::Name(extractor_name) = extractor {
            extract_video_url_with_extractor(url, extractor_name, None).await
        } else {
            extract_video_url(url, None).await
        };

        let extracted_video = match extractor_result {
            Some(Ok(video_url)) => video_url,
            Some(Err(err)) => {
                log::error!("Failed to extract video url: {:#}", err);
                return true;
            }
            None => {
                if let Extractor::Name(extractor_name) = extractor {
                    log::error!("Failed to find an extractor named: {}", extractor_name);
                } else {
                    log::error!("Failed to find an extractor for the url: {}", url);
                }
                return true;
            }
        };

        let timestamp = Local::now().format("%Y-%m-%d_%H:%M:%S.%3f").to_string();
        let mut i = 0;

        let output_path = loop {
            let name = if i == 0 {
                Cow::Borrowed(&timestamp)
            } else {
                Cow::Owned(format!("{}-{}", timestamp, i))
            };

            let mp4_exists = save_directory.join(format!("{}.mp4", name)).exists();
            let ts_exists = save_directory.join(format!("{}.ts", name)).exists(); // TODO: convert everything to try_exists

            if !mp4_exists && !ts_exists {
                break save_directory.join(name.deref());
            }

            i += 1;
        };

        let download_result = episodes_downloader
            .download_to_file(
                InternalDownloadTask::new(output_path, extracted_video.url)
                    .output_path_has_extension(false)
                    .referer(extracted_video.referer),
            )
            .await;

        if let Err(err) = download_result {
            log::error!("Failed download: {:#}", err);
            return true;
        }
    } else {
        let Some(series_downloader) = downloaders::find_downloader_for_url(chrome.unwrap(), url).await else {
            log::error!("Failed to find a downloader for the url: {}", url);
            return true;
        };

        let download_settings = args.get_download_settings();
        let series_info = match series_downloader.get_series_info().await {
            Ok(info) => info,
            Err(err) => {
                log::error!("Failed to get series info: {:#}", err);
                return true;
            }
        };

        let (download_manager, sender) =
            DownloadManager::new(episodes_downloader, max_concurrent, save_directory, series_info);
        let download_request = DownloadRequest {
            language: args.get_video_type(),
            episodes: args.get_episodes_request(),
        };

        let (downloader_result, _) = tokio::join!(
            series_downloader.download(download_request, &download_settings, sender),
            download_manager.progress_downloads(),
        );

        if let Err(err) = downloader_result {
            log::error!("Failed to download series: {:#}", err);
            return true;
        }
    }

    false
}
