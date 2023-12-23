#![cfg_attr(not(debug_assertions), allow(warnings, unused))]
use std::borrow::Cow;
use std::ops::Deref;
use std::path::PathBuf;

use chrono::Local;
use clap::Parser;
use cli::{Args, Extractor};
use download::{DownloadManager, Downloader, InternalDownloadTask};
use downloaders::{DownloadRequest, DownloadTask, InstantiatedDownloader};
use extractors::{extract_video_url, extract_video_url_with_extractor_from_url};
use ffmpeg::Ffmpeg;
use logger::log_wrapper::{LogWrapper, SetLogWrapper};
use tokio_stream::wrappers::UnboundedReceiverStream;

pub(crate) mod chrome;
pub(crate) mod cli;
pub(crate) mod dirs;
pub(crate) mod download;
pub(crate) mod downloaders;
pub(crate) mod extractors;
pub(crate) mod ffmpeg;
pub(crate) mod logger;
pub(crate) mod mpv;
pub(crate) mod utils;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Parse arguments
    let args = cli::Args::parse();
    let debug = args.debug;
    let url = args.url.deref();
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

    // Fail fast if extractor name or url is invalid
    if let Some(extractor) = extractor {
        let extractor_name = match extractor {
            Extractor::Auto => None,
            Extractor::Name(extractor_name) => {
                if !extractors::exists_extractor_with_name(extractor_name) {
                    log::error!("Failed to find an extractor named: {}", extractor_name);
                    std::process::exit(1);
                }

                Some(extractor_name.deref())
            }
        };

        if !extractors::exists_extractor_for_url(url, extractor_name).await {
            if let Some(extractor_name) = extractor_name {
                log::error!(
                    "The specified extractor \"{}\" does not support the url: {}",
                    extractors::normalized_name(extractor_name).unwrap(),
                    url
                );
            } else {
                log::error!("Failed to find an extractor for the url: {}", url);
            }

            std::process::exit(1);
        }
    } else if !downloaders::exists_downloader_for_url(url).await {
        log::error!("No downloader found for the url: {}", url);
        std::process::exit(1);
    }

    // Set up FFmpeg, and ChromeDriver if needed
    let asset_downloader = Downloader::new(&mut log_wrapper, debug, None, None, None);
    let ffmpeg = Ffmpeg::new(data_dir.clone());

    let (mut chrome, ffmpeg_install_result) = if extractor.is_none() {
        let chrome_ffmpeg_future = futures_util::future::join(
            chrome::ChromeDriver::get(&data_dir, &asset_downloader, !debug),
            ffmpeg.auto_download(&asset_downloader),
        );
        let (chrome, ffmpeg_install_result) = tokio::select! {
            biased;

            result = chrome_ffmpeg_future => result,
            _ = asset_downloader.tick() => unreachable!(),
        };

        let chrome = match chrome {
            Ok(chrome) => chrome,
            Err(err) => {
                log::error!("Failed to create ChromeDriver: {:#}", err);
                std::process::exit(1);
            }
        };

        (Some(chrome), ffmpeg_install_result)
    } else {
        let ffmpeg_install_result = tokio::select! {
            biased;

            result = ffmpeg.auto_download(&asset_downloader) => result,
            _ = asset_downloader.tick() => unreachable!(),
        };

        (None, ffmpeg_install_result)
    };

    // Do much of the bulk work
    let should_error_quit = do_after_chrome_driver(
        ffmpeg_install_result,
        asset_downloader,
        chrome.as_mut().map(|(chrome, _)| chrome),
        log_wrapper,
        save_directory,
        args,
    )
    .await;

    // Quit ChromeDriver
    if let Some((chrome, mut chrome_process)) = chrome {
        if let Err(err) = chrome.quit().await {
            log::warn!("Failed to quit ChromeDriver: {}", err);
        }

        if let Err(err) = chrome_process.kill() {
            log::warn!("Failed to kill ChromeDriver: {}", err);
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
    let max_concurrent = args.concurrent_downloads.inner().copied();

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
    let episodes_downloader = if !args.mpv {
        Some(Downloader::new(
            &mut log_wrapper,
            debug,
            Some(ffmpeg_path),
            user_agent,
            Some(args.retries.inner().copied()),
        ))
    } else {
        None
    };

    if let Some(extractor) = extractor {
        let extractor_result = if let Extractor::Name(extractor_name) = extractor {
            extract_video_url_with_extractor_from_url(url, extractor_name, None, None).await
        } else {
            extract_video_url(url, None, None).await
        };

        let extracted_video = match extractor_result {
            Some(Ok(video_url)) => video_url,
            Some(Err(err)) => {
                log::error!("Failed to extract video url: {:#}", err);
                return true;
            }
            None => unreachable!(),
        };

        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S.%3f").to_string();
        let mut i = 0u32;

        let output_path = loop {
            let name = if i == 0 {
                Cow::Borrowed(&timestamp)
            } else {
                Cow::Owned(format!("{}-{}", timestamp, i))
            };

            let mp4_name = format!("{}.mp4", name);
            let mp4_exists = match save_directory.join(&mp4_name).try_exists() {
                Ok(exists) => exists,
                Err(err) => {
                    log::error!("Failed to check if the file \"{}\" exists: {}", mp4_name, err);
                    return true;
                }
            };

            let ts_name = format!("{}.ts", name);
            let ts_exists = match save_directory.join(&ts_name).try_exists() {
                Ok(exists) => exists,
                Err(err) => {
                    log::error!("Failed to check if the file \"{}\" exists: {}", ts_name, err);
                    return true;
                }
            };

            if !mp4_exists && !ts_exists {
                break save_directory.join(name.deref());
            }

            match i.checked_add(1) {
                Some(next_i) => i = next_i,
                None => {
                    log::error!("Failed to find a name for the file");
                    return true;
                }
            }
        };

        let result = if let Some(episodes_downloader) = episodes_downloader {
            let download_future = episodes_downloader.download_to_file(
                InternalDownloadTask::new(output_path, extracted_video.url)
                    .output_path_has_extension(false)
                    .referer(extracted_video.referer),
            );

            tokio::select! {
                biased;

                result = download_future => result,
                _ = episodes_downloader.tick() => unreachable!(),
            }
        } else {
            mpv::start_mpv(&extracted_video.url, debug)
        };

        if let Err(err) = result {
            if !args.mpv {
                log::error!("Failed download: {:#}", err);
            } else {
                log::error!("Failed mpv: {:#}", err);
            }

            return true;
        }
    } else {
        let series_downloader = downloaders::find_downloader_for_url(chrome.unwrap(), url)
            .await
            .unwrap();
        let download_settings = args.get_download_settings();
        let series_info = match series_downloader.get_series_info().await {
            Ok(info) => info,
            Err(err) => {
                log::error!("Failed to get series info: {:#}", err);
                return true;
            }
        };

        let download_request = DownloadRequest {
            language: args.get_video_type(),
            episodes: args.get_episodes_request(),
        };

        if let Some(episodes_downloader) = episodes_downloader {
            let (download_manager, sender) =
                DownloadManager::new(episodes_downloader, max_concurrent, save_directory, series_info);

            let (downloader_result, _) = tokio::join!(
                series_downloader.download(download_request, download_settings, sender),
                download_manager.progress_downloads(),
            );

            if let Err(err) = downloader_result {
                log::error!("Failed to download series: {:#}", err);
                return true;
            }
        } else {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<DownloadTask>();
            let rx_stream = UnboundedReceiverStream::new(rx);

            let mpv_future = mpv::start_mpv_with_ipc(rx_stream, series_info, debug);
            tokio::pin!(mpv_future);

            let (downloader_errored, mpv_result) = tokio::select! {
                mpv_result = &mut mpv_future => (false, mpv_result),
                downloader_result = series_downloader.download(download_request, download_settings, tx) => {
                    if let Err(err) = &downloader_result {
                        log::error!("Failed to download series: {:#}", err);
                    }

                    (downloader_result.is_err(), mpv_future.await)
                }
            };

            if let Err(err) = &mpv_result {
                log::error!("Failed mpv: {:#}", err);
            }

            return downloader_errored || mpv_result.is_err();
        }
    }

    false
}
