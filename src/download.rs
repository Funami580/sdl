use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::Write;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use reqwest::IntoUrl;
use reqwest_partial_retry::{ClientExt, Config};
use reqwest_retry::policies::ExponentialBackoffBuilder;
use reqwest_retry::DefaultRetryableStrategy;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use url::Url;

use crate::downloaders::{DownloadTask, EpisodeInfo, EpisodeNumber, Language, SeriesInfo, VideoType};
use crate::logger::log_wrapper::SetLogWrapper;
use crate::utils::remove_file_ignore_not_exists;

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36";

static DEFAULT_RETRY_CLIENT: Lazy<reqwest_partial_retry::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .build()
        .unwrap()
        .resumable_with_config(
            Config::builder()
                .retry_policy(ExponentialBackoffBuilder::default().build_with_max_retries(5))
                .retryable_strategy(DefaultRetryableStrategy)
                .stream_timeout(Some(Duration::from_secs(60)))
                .build(),
        )
});

pub(crate) struct DownloadManager {
    downloader: Downloader,
    rx_stream: UnboundedReceiverStream<DownloadTask>,
    max_concurrent: Option<usize>,
    save_directory: PathBuf,
    series_info: SeriesInfo,
}

impl DownloadManager {
    pub(crate) fn new(
        downloader: Downloader,
        max_concurrent: Option<NonZeroU32>,
        save_directory: PathBuf,
        series_info: SeriesInfo,
    ) -> (Self, UnboundedSender<DownloadTask>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<DownloadTask>();
        let rx_stream = UnboundedReceiverStream::new(rx);

        let manager = DownloadManager {
            downloader,
            rx_stream,
            max_concurrent: max_concurrent.map(|n| n.get() as usize),
            save_directory,
            series_info,
        };

        (manager, tx)
    }

    pub(crate) async fn progress_downloads(self) {
        let anime_name_for_file = prepare_series_name_for_file(&self.series_info.title);
        let download_future = self
            .rx_stream
            .for_each_concurrent(self.max_concurrent, |download_task| {
                let output_name = get_episode_name(
                    anime_name_for_file.as_deref(),
                    Some(&download_task.language),
                    &download_task.episode_info,
                    false,
                );
                let output_path_no_extension = self.save_directory.join(&output_name);

                let internal_task = InternalDownloadTask::new(output_path_no_extension, download_task.download_url)
                    .output_path_has_extension(false)
                    .referer(download_task.referer);
                let downloader_borrowed = &self.downloader;

                async move {
                    if let Err(err) = downloader_borrowed.download_to_file(internal_task).await {
                        log::warn!("Failed download of {}: {:#}", output_name, err);
                    }
                }
            });

        tokio::select! {
            biased;

            _ = download_future => {}
            _ = self.downloader.tick() => unreachable!()
        }
    }
}

pub(crate) struct InternalDownloadTask {
    url: String,
    output_path: PathBuf,
    output_path_has_extension: bool,
    overwrite_file: bool,
    custom_message: Option<String>,
    referer: Option<String>,
}

impl InternalDownloadTask {
    pub(crate) fn new(output_path: PathBuf, url: String) -> Self {
        InternalDownloadTask {
            url,
            output_path,
            output_path_has_extension: true,
            overwrite_file: false,
            custom_message: None,
            referer: None,
        }
    }

    pub(crate) fn output_path_has_extension(mut self, output_path_has_extension: bool) -> Self {
        self.output_path_has_extension = output_path_has_extension;
        self
    }

    pub(crate) fn overwrite_file(mut self, overwrite_file: bool) -> Self {
        self.overwrite_file = overwrite_file;
        self
    }

    pub(crate) fn custom_message(mut self, custom_message: Option<String>) -> Self {
        self.custom_message = custom_message;
        self
    }

    pub(crate) fn referer(mut self, referer: Option<String>) -> Self {
        self.referer = referer;
        self
    }
}

enum ProgressBarOrResult {
    ProgressBar(indicatif::ProgressBar),
    Abandoned { position: u64, length: Option<u64> },
    Finished { length: u64 },
}

impl ProgressBarOrResult {
    fn is_finished(&self) -> bool {
        match self {
            ProgressBarOrResult::ProgressBar(pb) => pb.is_finished(),
            ProgressBarOrResult::Abandoned { position: _, length: _ } => false,
            ProgressBarOrResult::Finished { length: _ } => true,
        }
    }

    fn position(&self) -> u64 {
        match self {
            ProgressBarOrResult::ProgressBar(pb) => pb.position(),
            ProgressBarOrResult::Abandoned { position, length: _ } => *position,
            ProgressBarOrResult::Finished { length } => *length,
        }
    }

    fn length(&self) -> Option<u64> {
        match self {
            ProgressBarOrResult::ProgressBar(pb) => pb.length(),
            ProgressBarOrResult::Abandoned { position: _, length } => *length,
            ProgressBarOrResult::Finished { length } => Some(*length),
        }
    }
}

pub(crate) struct Downloader {
    client: Option<reqwest_partial_retry::Client>,
    multi_progress: indicatif::MultiProgress,
    total_progress: RefCell<Option<indicatif::ProgressBar>>,
    sub_progresses: RefCell<Vec<ProgressBarOrResult>>,
    ffmpeg_path: Option<PathBuf>,
    user_agent: Option<String>,
    debug: bool,
}

impl Downloader {
    pub(crate) fn new(
        log_wrapper: &mut SetLogWrapper,
        debug: bool,
        ffmpeg_path: Option<PathBuf>,
        user_agent: Option<String>,
        retries: Option<Option<NonZeroU32>>,
    ) -> Self {
        let multi_progress = indicatif::MultiProgress::new();
        log_wrapper.set_multi(Some(multi_progress.clone()));

        let client = if let Some(retries) = retries {
            let client = reqwest::Client::builder()
                .user_agent(DEFAULT_USER_AGENT)
                .connect_timeout(Duration::from_secs(20))
                .build()
                .unwrap()
                .resumable_with_config(
                    Config::builder()
                        .retry_policy(
                            ExponentialBackoffBuilder::default()
                                .build_with_max_retries(retries.map(|x| x.get()).unwrap_or(u32::MAX)),
                        )
                        .retryable_strategy(DefaultRetryableStrategy)
                        .stream_timeout(Some(Duration::from_secs(60)))
                        .build(),
                );

            Some(client)
        } else {
            None
        };

        Downloader {
            client,
            multi_progress,
            total_progress: RefCell::new(None),
            sub_progresses: RefCell::new(vec![]),
            ffmpeg_path,
            user_agent,
            debug,
        }
    }

    pub(crate) async fn download_to_file(&self, task: InternalDownloadTask) -> Result<(), anyhow::Error> {
        let url = Url::parse(&task.url).with_context(|| "failed to parse URL")?;
        let response = get_response(
            self.client.as_ref(),
            url.clone(),
            self.user_agent.as_deref(),
            task.referer.as_deref(),
        )
        .await?;
        let is_m3u8 = is_m3u8_url(response.url());

        let output_path = if !task.output_path_has_extension {
            match (
                task.output_path.parent(),
                task.output_path.file_name().map(|file_name| file_name.to_owned()),
            ) {
                (Some(parent), Some(mut file_name)) => {
                    let extension = if is_m3u8 { ".ts" } else { ".mp4" };
                    file_name.push(extension);
                    parent.join(file_name)
                }
                _ => task.output_path,
            }
        } else {
            task.output_path
        };

        let message = if let Some(custom_message) = task.custom_message {
            custom_message
        } else {
            let final_path = if is_m3u8 {
                Cow::Owned(output_path.with_extension("mp4"))
            } else {
                Cow::Borrowed(&output_path)
            };

            final_path
                .file_name()
                .with_context(|| "failed to get file name")?
                .to_string_lossy()
                .to_string()
        };

        let target_file = if task.overwrite_file {
            tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&output_path)
                .await
        } else {
            tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)
                .await
        }
        .with_context(|| "failed to open download target file")?;

        if is_m3u8 {
            self.m3u8_download(
                response,
                task.referer.as_deref(),
                url,
                target_file,
                output_path,
                message,
            )
            .await
        } else {
            self.simple_download(response, target_file, message).await
        }
    }

    async fn simple_download(
        &self,
        response: reqwest_partial_retry::ResumableResponse,
        target_file: tokio::fs::File,
        message: String,
    ) -> Result<(), anyhow::Error> {
        let content_length = response.content_length();

        let (sub_progresses_index, progress_bar) = if let Some(content_length) = content_length {
            self.create_progress_bar(message, content_length)
        } else {
            self.create_progress_bar_unknown_bytes(message)
        };

        let mut input_stream = response.bytes_stream_resumable();
        let mut output_stream = tokio::io::BufWriter::new(target_file);
        let mut downloaded = 0;

        while let Some(item) = input_stream.next().await {
            let mut chunk = match item {
                Ok(chunk) => chunk,
                Err(err) => {
                    self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                    return Err(err).with_context(|| "failed download");
                }
            };

            downloaded += chunk.len() as u64;

            if let Err(err) = output_stream.write_all_buf(&mut chunk).await {
                self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                return Err(err).with_context(|| "failed writing to download file");
            }

            self.update_progress(&progress_bar, downloaded, content_length);
        }

        if let Err(err) = Self::clean_up_write(output_stream).await {
            self.clean_up_progress_bar(&progress_bar, sub_progresses_index);
            return Err(err);
        }

        self.clean_up_progress_bar(&progress_bar, sub_progresses_index);

        Ok(())
    }

    async fn m3u8_download(
        &self,
        response: reqwest_partial_retry::ResumableResponse,
        referer: Option<&str>,
        m3u8_url: Url,
        target_file: tokio::fs::File,
        target_path: PathBuf,
        message: String,
    ) -> Result<(), anyhow::Error> {
        let m3u8_bytes = get_response_bytes(response.response()).await?;

        let media_playlist = match m3u8_rs::parse_playlist_res(&m3u8_bytes) {
            Ok(m3u8_rs::Playlist::MasterPlaylist(mut playlist)) => {
                if playlist.variants.is_empty() {
                    anyhow::bail!("could not find any media playlists");
                }

                let highest_quality_variant = playlist
                    .variants
                    .select_nth_unstable_by(0, |a, b| {
                        use std::cmp::Ordering;

                        match (a.is_i_frame, b.is_i_frame) {
                            (true, true) => return Ordering::Equal,
                            (true, false) => return Ordering::Greater,
                            (false, true) => return Ordering::Less,
                            (false, false) => {}
                        }

                        if let (Some(res_a), Some(res_b)) = (a.resolution, b.resolution) {
                            let res_a_pixels = res_a.width * res_a.height;
                            let res_b_pixels = res_b.width * res_b.height;

                            if res_a_pixels != res_b_pixels {
                                return res_a_pixels.cmp(&res_b_pixels).reverse();
                            }
                        }

                        if let (Some(bw_a), Some(bw_b)) = (a.average_bandwidth, b.average_bandwidth) {
                            return bw_a.cmp(&bw_b).reverse();
                        }

                        a.bandwidth.cmp(&b.bandwidth).reverse()
                    })
                    .1;

                if highest_quality_variant.is_i_frame {
                    anyhow::bail!("could not find a non-iframe media playlist");
                }

                let media_playlist_url = m3u8_url
                    .join(&highest_quality_variant.uri)
                    .with_context(|| "failed to create m3u8 media playlist url")?;
                let m3u8_media_bytes = get_page_bytes(media_playlist_url, self.user_agent.as_deref(), referer).await?;

                match m3u8_rs::parse_media_playlist_res(&m3u8_media_bytes) {
                    Ok(media_playlist) => media_playlist,
                    Err(_) => anyhow::bail!("failed to parse m3u8 media playlist"),
                }
            }
            Ok(m3u8_rs::Playlist::MediaPlaylist(playlist)) => {
                if playlist.i_frames_only {
                    anyhow::bail!("is iframe media playlist");
                }

                playlist
            }
            Err(_) => anyhow::bail!("failed to parse m3u8"),
        };

        let (sub_progresses_index, progress_bar) = self.create_progress_bar(message, u64::MAX);
        let mut output_stream = tokio::io::BufWriter::new(target_file);
        let mut downloaded_bytes = 0;
        let total_duration: f64 = media_playlist
            .segments
            .iter()
            .map(|segment| segment.duration as f64)
            .sum();
        let mut downloaded_duration: f64 = 0.0;
        let mut total_bytes_estimation = None;

        for segment in media_playlist.segments {
            let segment_url = match m3u8_url.join(&segment.uri) {
                Ok(segment_url) => segment_url,
                Err(err) => {
                    self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                    return Err(err).with_context(|| "failed to create m3u8 segment url");
                }
            };
            let response =
                match get_response(self.client.as_ref(), segment_url, self.user_agent.as_deref(), referer).await {
                    Ok(response) => response,
                    Err(err) => {
                        self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                        return Err(err).with_context(|| "failed to get segment response");
                    }
                };
            let mut input_stream = response.bytes_stream_resumable();

            while let Some(item) = input_stream.next().await {
                let mut chunk = match item {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                        return Err(err).with_context(|| "failed download");
                    }
                };

                downloaded_bytes += chunk.len() as u64;

                if let Err(err) = output_stream.write_all_buf(&mut chunk).await {
                    self.error_cleanup_progress_bar(&progress_bar, sub_progresses_index);
                    return Err(err).with_context(|| "failed writing to download file");
                }

                self.update_progress(&progress_bar, downloaded_bytes, total_bytes_estimation);
            }

            downloaded_duration += segment.duration as f64;
            total_bytes_estimation =
                Some(((downloaded_bytes as f64 * total_duration) / downloaded_duration).ceil() as u64);
        }

        if let Err(err) = Self::clean_up_write(output_stream).await {
            self.clean_up_progress_bar(&progress_bar, sub_progresses_index);
            return Err(err);
        }

        if let Some(ffmpeg_path) = &self.ffmpeg_path {
            let mut ffmpeg_cmd = tokio::process::Command::new(ffmpeg_path);

            if !self.debug {
                ffmpeg_cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }

            let ffmpeg_spawn_result = ffmpeg_cmd
                .arg("-i")
                .arg(&target_path)
                .arg("-c")
                .arg("copy")
                .arg(target_path.with_extension("mp4"))
                .spawn();

            match ffmpeg_spawn_result {
                Ok(mut child) => match child.wait().await {
                    Ok(ffmpeg_result) => match ffmpeg_result.code() {
                        Some(code) if code != 0 => log::warn!("FFmpeg failed with exit code {}", code),
                        None => log::warn!("FFmpeg failed due to signal termination"),
                        _ => {
                            if let Err(err) = remove_file_ignore_not_exists(&target_path).await {
                                log::warn!("Failed to delete temporary input file for FFmpeg: {}", err);
                            }
                        }
                    },
                    Err(err) => {
                        log::warn!("FFmpeg was not running: {}", err);
                    }
                },
                Err(err) => {
                    log::warn!("Failed to start FFmpeg: {}", err);
                }
            }
        } else {
            let temp_name = target_path
                .file_name()
                .unwrap_or(target_path.as_os_str())
                .to_string_lossy();
            log::info!(
                "Failed to convert \"{}\" to MP4 due to FFmpeg not being installed",
                temp_name
            );
        }

        self.clean_up_progress_bar(&progress_bar, sub_progresses_index);

        Ok(())
    }

    async fn clean_up_write(mut output_stream: tokio::io::BufWriter<tokio::fs::File>) -> Result<(), anyhow::Error> {
        if let Err(err) = output_stream.flush().await {
            return Err(err).with_context(|| "failed flushing to download file");
        }

        if let Err(err) = output_stream.get_mut().sync_all().await {
            return Err(err).with_context(|| "failed syncing download file to disk");
        }

        Ok(())
    }

    fn update_progress(&self, progress_bar: &indicatif::ProgressBar, downloaded: u64, total_bytes: Option<u64>) {
        progress_bar.update(|state| {
            if !(state.len() == Some(u64::MAX) && total_bytes.is_none()) {
                state.set_len(total_bytes.unwrap_or(0).max(downloaded));
            }

            state.set_pos(downloaded);
        });

        self.update_progress_total(true, true)
    }

    fn update_progress_total(&self, bytes: bool, message: bool) {
        if !bytes && !message {
            return;
        }

        let sub_progresses_lock = self.sub_progresses.borrow();

        let updated_bytes = if bytes {
            let total_downloaded: u64 = sub_progresses_lock
                .iter()
                .map(|pb| pb.position())
                .reduce(|acc, e| acc.saturating_add(e))
                .unwrap();
            let total_length: u64 = sub_progresses_lock
                .iter()
                .filter_map(|pb| pb.length())
                .reduce(|acc, e| acc.saturating_add(e))
                .unwrap();
            Some((total_downloaded, total_length))
        } else {
            None
        };

        let updated_message = if message {
            let total_finished = sub_progresses_lock.iter().filter(|pb| pb.is_finished()).count();
            let total_bars = sub_progresses_lock.len();
            Some(format!("Total {total_finished}/{total_bars}"))
        } else {
            None
        };

        drop(sub_progresses_lock);

        let total_progress_lock = self.total_progress.borrow();
        let total_progress = total_progress_lock.as_ref().unwrap();

        if let Some((total_downloaded, total_length)) = updated_bytes {
            total_progress.update(|state| {
                state.set_len(total_length);
                state.set_pos(total_downloaded);
            });
        }

        if let Some(message) = updated_message {
            total_progress.set_message(message);
        }

        drop(total_progress_lock);
    }

    fn create_total_progress_bar() -> indicatif::ProgressBar {
        indicatif::ProgressBar::new(u64::MAX)
            .with_style(custom_progress_style(
                indicatif::ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_msg} {binary_bytes_per_sec:>14} {bytes:>10}{total_bytes:<11} [{bar}] {eta:>5} {percent:>3}%",
                )
                .unwrap()
            ))
            .with_message("Total 0/1")
    }

    fn create_progress_bar(&self, name: String, bytes: u64) -> (usize, indicatif::ProgressBar) {
        let pb = indicatif::ProgressBar::new(bytes)
            .with_style(custom_progress_style(
                indicatif::ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_msg} {binary_bytes_per_sec:>14} {bytes:>10}{total_bytes:<11} [{bar}] {eta:>5} {percent:>3}%",
                )
                .unwrap()
            ))
            .with_message(name);
        self.post_prepare_progress_bar(pb)
    }

    fn create_progress_bar_unknown_bytes(&self, name: String) -> (usize, indicatif::ProgressBar) {
        let pb = indicatif::ProgressBar::new(0)
            .with_style(custom_progress_style(
                indicatif::ProgressStyle::with_template(
                    "[{elapsed_precise}] {wide_msg} {binary_bytes_per_sec:>14} {bytes:>10}",
                )
                .unwrap(),
            ))
            .with_message(name);
        self.post_prepare_progress_bar(pb)
    }

    fn post_prepare_progress_bar(&self, progress_bar: indicatif::ProgressBar) -> (usize, indicatif::ProgressBar) {
        let mut total_progress_lock = self.total_progress.borrow_mut();

        if total_progress_lock.is_none() {
            let new_total_progress = Self::create_total_progress_bar();
            *total_progress_lock = Some(self.multi_progress.add(new_total_progress));
        }

        let pb = self
            .multi_progress
            .insert_before(total_progress_lock.as_ref().unwrap(), progress_bar);

        drop(total_progress_lock);

        let mut sub_progresses_lock = self.sub_progresses.borrow_mut();
        let sub_progresses_index = sub_progresses_lock.len();
        sub_progresses_lock.push(ProgressBarOrResult::ProgressBar(pb.clone()));
        drop(sub_progresses_lock);

        pb.tick();
        self.update_progress_total(true, true);

        (sub_progresses_index, pb)
    }

    fn clean_up_progress_bar(&self, progress_bar: &indicatif::ProgressBar, sub_progresses_index: usize) {
        progress_bar.finish();

        let position = progress_bar.position();
        let mut sub_progresses_lock = self.sub_progresses.borrow_mut();
        sub_progresses_lock[sub_progresses_index] = ProgressBarOrResult::Finished { length: position };
        drop(sub_progresses_lock);

        self.update_progress_total(true, true);
    }

    fn error_cleanup_progress_bar(&self, progress_bar: &indicatif::ProgressBar, sub_progresses_index: usize) {
        progress_bar.abandon();

        let position = progress_bar.position();
        let length = progress_bar.length();
        let mut sub_progresses_lock = self.sub_progresses.borrow_mut();
        sub_progresses_lock[sub_progresses_index] = ProgressBarOrResult::Abandoned { position, length };
        drop(sub_progresses_lock);
    }

    fn clean_up_total_progress_bar(&self) {
        if let Some(total_progress) = self.total_progress.borrow().deref() {
            let all_finished = self.sub_progresses.borrow().iter().all(|pb| pb.is_finished());

            if all_finished {
                total_progress.finish();
            } else {
                total_progress.abandon();
            }
        }
    }

    /// This function never finishes. It should be used in a select! expression.
    pub(crate) async fn tick(&self) {
        const TICK_INTERVAL: Duration = Duration::from_millis(100);

        loop {
            for sub_progress in self.sub_progresses.borrow().deref() {
                if let ProgressBarOrResult::ProgressBar(pb) = &sub_progress {
                    if !pb.is_finished() {
                        pb.tick();
                    }
                }
            }

            if let Some(total_progress) = self.total_progress.borrow().deref() {
                if !total_progress.is_finished() {
                    total_progress.tick();
                }
            }

            tokio::time::sleep(TICK_INTERVAL).await;
        }
    }

    pub(crate) fn clear(self) {
        self.clean_up_total_progress_bar();
        drop(self.total_progress.take());
        let _ = self.multi_progress.clear();
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        self.clean_up_total_progress_bar();
    }
}

fn custom_progress_style(progress_style: indicatif::ProgressStyle) -> indicatif::ProgressStyle {
    use indicatif::{HumanDuration, ProgressState};
    use number_prefix::NumberPrefix;

    progress_style
        .with_key("bytes", |state: &ProgressState, w: &mut dyn Write| {
            let _ = match NumberPrefix::binary(state.pos() as f64) {
                NumberPrefix::Standalone(number) => write!(w, "{number:.0} B"),
                NumberPrefix::Prefixed(prefix, number) => write!(w, "{number:.1} {prefix}B"),
            };
        })
        .with_key("total_bytes", |state: &ProgressState, w: &mut dyn Write| {
            // Only if total bytes are known
            if state.len() != Some(u64::MAX) {
                let _ = write!(w, "/");
                let _ = match NumberPrefix::binary(state.len().unwrap() as f64) {
                    NumberPrefix::Standalone(number) => write!(w, "{number:.0} B"),
                    NumberPrefix::Prefixed(prefix, number) => write!(w, "{number:.1} {prefix}B"),
                };
            }
        })
        .with_key("binary_bytes_per_sec", |state: &ProgressState, w: &mut dyn Write| {
            let _ = match NumberPrefix::binary(state.per_sec()) {
                NumberPrefix::Standalone(number) => write!(w, "{number:.0} B/s"),
                NumberPrefix::Prefixed(prefix, number) => write!(w, "{number:.1} {prefix}B/s"),
            };
        })
        .with_key("percent", |state: &ProgressState, w: &mut dyn Write| {
            let mut percent = ((state.fraction() * 100.0).floor() as u32).clamp(0, 100);

            if percent == 100 && !state.is_finished() {
                percent = 99;
            }

            let _ = write!(w, "{}", percent);
        })
        .with_key("bar", |state: &ProgressState, w: &mut dyn Write| {
            const BAR_WIDTH: usize = 40;
            const FILLED_STR: &str = "#";
            const IN_PROGRESS_STR: &str = ">";
            const TODO_STR: &str = "-";

            let mut filled = ((state.fraction() * BAR_WIDTH as f32) as usize).min(BAR_WIDTH);

            if filled == BAR_WIDTH && !state.is_finished() {
                filled = BAR_WIDTH - 1;
            }

            let non_filled = BAR_WIDTH - filled;
            let in_progress = 1.min(non_filled);
            let todo = BAR_WIDTH - filled - in_progress;

            let _ = write!(
                w,
                "{}{}{}",
                console::style(FILLED_STR.repeat(filled)).cyan(),
                console::style(IN_PROGRESS_STR.repeat(in_progress)).cyan(),
                console::style(TODO_STR.repeat(todo)).blue(),
            );
        })
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
            // Only if total bytes are known
            if state.len() != Some(u64::MAX) {
                let _ = write!(w, "({:#})", HumanDuration(state.eta()));
            }
        })
}

pub(crate) async fn get_response<U: IntoUrl>(
    client: Option<&reqwest_partial_retry::Client>,
    url: U,
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Result<reqwest_partial_retry::ResumableResponse, anyhow::Error> {
    let client = client.unwrap_or(DEFAULT_RETRY_CLIENT.deref());
    let mut request = client.get(url);

    if let Some(user_agent) = user_agent {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }

    if let Some(referer) = referer {
        request = request.header(reqwest::header::REFERER, referer);
    }

    let response = client
        .execute_resumable(request.build().with_context(|| "failed to build request")?)
        .await
        .with_context(|| "failed to request url")?;

    Ok(response)
}

pub(crate) async fn get_response_bytes(response: reqwest::Response) -> Result<bytes::Bytes, anyhow::Error> {
    response
        .bytes()
        .await
        .with_context(|| "failed to get bytes of response body")
}

pub(crate) async fn get_page_bytes<U: IntoUrl>(
    url: U,
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Result<bytes::Bytes, anyhow::Error> {
    get_response_bytes(get_response(None, url, user_agent, referer).await?.response()).await
}

pub(crate) async fn get_page_text<U: IntoUrl>(
    url: U,
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Result<String, anyhow::Error> {
    get_response(None, url, user_agent, referer)
        .await?
        .response()
        .text()
        .await
        .with_context(|| "failed to parse response body as text")
}

pub(crate) async fn get_page_json<U: IntoUrl>(
    url: U,
    user_agent: Option<&str>,
    referer: Option<&str>,
) -> Result<serde_json::Value, anyhow::Error> {
    get_response(None, url, user_agent, referer)
        .await?
        .response()
        .json()
        .await
        .with_context(|| "failed to parse response body as json")
}

fn is_m3u8_url(url: &Url) -> bool {
    url.path_segments()
        .and_then(|segments| segments.last())
        .map(|last| {
            let lower = last.to_ascii_lowercase();
            (lower.ends_with(".m3u8") && lower.len() != ".m3u8".len())
                || (lower.ends_with(".m3u") && lower.len() != ".m3u".len())
        })
        .unwrap_or(false)
}

fn prepare_series_name_for_file(name: &str) -> Option<String> {
    use regex::Regex;

    const NAME_LIMIT: usize = 160;

    let no_control_chars = name.replace(|c: char| c.is_ascii_control(), "");
    let no_special_spaces = no_control_chars.replace(char::is_whitespace, " ");
    let no_quotes = no_special_spaces.replace('\"', "");

    static COLON_V1_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\p{LETTER}[:digit:]]): +([\p{LETTER}[:digit:]])").unwrap());
    static COLON_V2_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\p{LETTER}[:digit:]]):([\p{LETTER}[:digit:]])").unwrap());
    let no_colon = COLON_V1_REGEX.replace_all(&no_quotes, r"${1} - ${2}");
    let no_colon = COLON_V2_REGEX.replace_all(&no_colon, r"${1} ${2}");
    let no_colon = no_colon.replace(':', "");

    static QUESTION_MARKS_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\p{LETTER}[:digit:]])\?+ +([\p{LETTER}[:digit:]])").unwrap());
    let no_question_marks = QUESTION_MARKS_REGEX.replace_all(&no_colon, r"${1} - ${2}");
    let no_question_marks = no_question_marks.replace('?', "");

    static SLASH_V1_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\b([\p{LETTER}[:digit:]])/+([\p{LETTER}[:digit:]])\b").unwrap());
    static SLASH_V2_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\p{LETTER}[:digit:]])/+([\p{LETTER}[:digit:]])").unwrap());
    let no_slashs = SLASH_V1_REGEX.replace_all(&no_question_marks, r"${1}${2}");
    let no_slashs = SLASH_V2_REGEX.replace_all(&no_slashs, r"${1} ${2}");
    let no_slashs = no_slashs.replace('/', "");

    let no_extra = no_slashs.replace(['\\', '*', '<', '>', '|'], "");

    static MULTIPLE_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());
    let no_multiple_space = MULTIPLE_SPACE.replace_all(&no_extra, " ");
    let no_dot_or_space_at_ends = no_multiple_space.trim_matches(|c: char| c == ' ' || c == '.');

    // Not needed, because we still append something to the filename
    //
    // static WINDOWS_RESERVED_REGEX: Lazy<Regex> = Lazy::new(||
    // Regex::new(r"^(?:con|prn|aux|nul|com\d|lpt\d)$").unwrap());
    // let no_reserved = if
    // WINDOWS_RESERVED_REGEX.is_match(&no_dot_or_space_at_ends) {     format!("
    // {no_dot_or_space_at_ends}_") } else {
    //     no_dot_or_space_at_ends.to_owned()
    // };

    if no_dot_or_space_at_ends.is_empty() {
        None
    } else {
        let mut total_bytes = 0;

        Some(
            no_dot_or_space_at_ends
                .chars()
                .take_while(|c| {
                    total_bytes += c.len_utf8();
                    total_bytes <= NAME_LIMIT
                })
                .collect(),
        )
    }
}

pub(crate) fn get_episode_name(
    anime_name: Option<&str>,
    language: Option<&VideoType>,
    episode_info: &EpisodeInfo,
    include_title_if_possible: bool,
) -> String {
    let mut output_name = String::new();

    if let Some(anime_name) = anime_name {
        output_name.push_str(anime_name);
        output_name.push_str(" - ");
    }

    if let Some(season) = episode_info.season_number {
        output_name.push_str(&format!("S{:02}", season));
    }

    let alignment_episode_number = episode_info
        .max_episode_number_in_season
        .map(|max_num| (max_num.checked_ilog10().unwrap_or(0) + 1) as usize);

    output_name.push('E');
    output_name.push_str(&format_episode_number(
        &episode_info.episode_number,
        alignment_episode_number,
    ));

    if let Some(language) = language {
        if language != &VideoType::Unspecified(Language::Unspecified) {
            output_name.push_str(&format!(" - {}", language));
        }
    }

    if include_title_if_possible {
        if let Some(title) = &episode_info.name {
            output_name.push_str(&format!(" - {}", title));
        }
    }

    output_name
}

fn format_episode_number(episode_number: &EpisodeNumber, alignment_episode_number: Option<usize>) -> String {
    match episode_number {
        EpisodeNumber::Number(episode_number) => {
            format!("{episode_number:0>fill$}", fill = alignment_episode_number.unwrap_or(2))
        }
        EpisodeNumber::String(episode_number) => {
            let trimmed_episode_number = episode_number.trim();

            if let Some((pre, post)) = trimmed_episode_number.split_once(['.', ',']) {
                let pre_all_digits = pre.bytes().all(|b| b.is_ascii_digit());
                let post_all_digits = post.bytes().all(|b| b.is_ascii_digit());

                if pre_all_digits && post_all_digits {
                    let delim = trimmed_episode_number.as_bytes()[pre.len()] as char;
                    return format!(
                        "{pre:0>fill$}{delim}{post}",
                        fill = alignment_episode_number.unwrap_or(2)
                    );
                }
            }

            trimmed_episode_number.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::download::format_episode_number;
    use crate::downloaders::EpisodeNumber;

    #[test]
    fn test_fix_filename() {
        use super::prepare_series_name_for_file;

        let comparisons = [
            (
                "The \"Hentai\" Prince and the Stony Cat",
                "The Hentai Prince and the Stony Cat",
            ),
            (
                "Anti Magic Academy: Test-Trupp 35",
                "Anti Magic Academy - Test-Trupp 35",
            ),
            (".hack//SIGN", "hack SIGN"),
            ("Code:Breaker", "Code Breaker"),
            ("Z/X Code reunion", "ZX Code reunion"),
            ("So I’m a Spider, So What?", "So I’m a Spider, So What"),
            ("<TEST>", "TEST"),
            ("Test | Hero", "Test Hero"),
            (" . . . . \0.\r.\t.\n Test*...", "Test"),
            ("/////Test/////", "Test"),
            ("Test1  Test2", "Test1 Test2"),
            ("Hacker\\MAN", "HackerMAN"),
            (
                "Sword Oratoria: Is it Wrong to Try to Pick Up Girls in a Dungeon? On the Side",
                "Sword Oratoria - Is it Wrong to Try to Pick Up Girls in a Dungeon - On the Side",
            ),
            (
                "Fate/Grand Order Absolute Demonic Front: Babylonia",
                "Fate Grand Order Absolute Demonic Front - Babylonia",
            ),
        ];

        for (input, expected) in comparisons {
            assert_eq!(
                prepare_series_name_for_file(input),
                Some(expected.to_owned()),
                "failed for {}",
                input
            );
        }
    }

    #[test]
    fn test_format_episode_number() {
        let tests = [
            ((EpisodeNumber::Number(5), None), "05"),
            ((EpisodeNumber::Number(15), None), "15"),
            ((EpisodeNumber::Number(5), Some(2)), "05"),
            ((EpisodeNumber::Number(15), Some(2)), "15"),
            ((EpisodeNumber::Number(15), Some(4)), "0015"),
            ((EpisodeNumber::String("15.5".to_string()), None), "15.5"),
            ((EpisodeNumber::String("15.5".to_string()), Some(4)), "0015.5"),
            ((EpisodeNumber::String("1000.5".to_string()), Some(4)), "1000.5"),
            ((EpisodeNumber::String("1.2.3".to_string()), None), "1.2.3"),
            ((EpisodeNumber::String("1.2.3".to_string()), Some(100)), "1.2.3"),
        ];

        for (input, output) in tests {
            assert_eq!(format_episode_number(&input.0, input.1), output.to_string());
        }
    }
}
