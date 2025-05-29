use std::fmt::Display;
use std::num::NonZeroU32;
use std::ops::{Deref, RangeInclusive};
use std::time::Duration;

use clap::ValueEnum;
use enum_dispatch::enum_dispatch;
use enum_iterator::Sequence;
use tokio::sync::mpsc::UnboundedSender;

use self::aniworldserienstream::AniWorldSerienStream;
use crate::extractors::ExtractedVideo;

pub mod aniworldserienstream;

macro_rules! enum_dispatch {
    {
        $(#[$meta:meta])*
        $vis:vis enum $name:ident$(<$($lt:lifetime),+>)?: $trait:ident $(+ $add_trait:ident)* {
            $($ty:ident$(<$($item_lt:lifetime)+>)?),*$(,)?
        }
    } => {
        #[enum_dispatch($trait$(, $add_trait)*)]
        $(#[$meta])*
        $vis enum $name$(<$($lt),+>)? {
            $($ty($ty$(<$($item_lt)*>)?),)*
        }
    }
}

macro_rules! exists_downloader_for_url {
    ($url:expr, $dl:ident $(, $tail:ident)* $(,)?) => {
        if <$dl>::supports_url($url).await {
            true
        } else {
            exists_downloader_for_url!($url, $($tail),*)
        }
    };
    ($url:expr $(,)?) => {
        false
    };
}

macro_rules! find_downloader_for_url {
    ($driver:expr, $browser_visible:expr, $url:expr, $dl:ident $(, $tail:ident)* $(,)?) => {
        if <$dl>::supports_url($url).await {
            Some(DispatchDownloader::from(<$dl>::new($driver, $browser_visible, $url.to_owned())))
        } else {
            find_downloader_for_url!($driver, $browser_visible, $url, $($tail),*)
        }
    };
    ($driver:expr, $browser_visible:expr, $url:expr $(,)?) => {
        None
    };
}

macro_rules! create_functions_for_extractors {
    ($( $dl:ident ),* $(,)?) => {
        enum_dispatch! {
            pub enum DispatchDownloader<'a>: InstantiatedDownloader {
                $($dl<'a>),*
            }
        }

        pub async fn exists_downloader_for_url(url: &str) -> bool {
            exists_downloader_for_url!(url, $($dl),*)
        }

        pub async fn find_downloader_for_url<'driver>(
            driver: &'driver thirtyfour::WebDriver,
            browser_visible: bool,
            url: &str,
        ) -> Option<DispatchDownloader<'driver>> {
            find_downloader_for_url!(driver, browser_visible, url, $($dl),*)
        }
    };
    () => {};
}

create_functions_for_extractors! {
    AniWorldSerienStream,
}

#[derive(Debug, Clone)]
pub struct SeriesInfo {
    pub title: String,
    pub description: Option<String>,
    pub status: Option<SeriesStatus>,
    pub year: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesStatus {
    Airing,
    Completed,
    OnHiatus,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoType {
    Unspecified(Language),
    Raw,
    Dub(Language),
    Sub(Language),
}

impl VideoType {
    pub fn get_language(&self) -> Option<&Language> {
        let language = match self {
            VideoType::Unspecified(language) => language,
            VideoType::Raw => return None,
            VideoType::Dub(language) => language,
            VideoType::Sub(language) => language,
        };

        Some(language)
    }

    pub fn convert_to_non_unspecified_video_types<'a>(
        &'a self,
        supported_video_types: &'a [VideoType],
    ) -> Vec<&'a VideoType> {
        let mut no_unspecified = supported_video_types.iter().filter(|video_type| match video_type {
            VideoType::Unspecified(_) => false,
            VideoType::Dub(Language::Unspecified) => false,
            VideoType::Sub(Language::Unspecified) => false,
            _ => true,
        });

        match self {
            VideoType::Unspecified(Language::Unspecified) => no_unspecified.collect(),
            VideoType::Unspecified(language) => no_unspecified
                .filter(|video_type| video_type.get_language() == Some(language))
                .collect(),
            VideoType::Dub(Language::Unspecified) => no_unspecified
                .filter(|video_type| matches!(video_type, VideoType::Dub(_)))
                .collect(),
            VideoType::Sub(Language::Unspecified) => no_unspecified
                .filter(|video_type| matches!(video_type, VideoType::Sub(_)))
                .collect(),
            other_type => {
                if no_unspecified.any(|video_type| video_type == other_type) {
                    vec![other_type]
                } else {
                    vec![]
                }
            }
        }
    }

    pub fn convert_to_non_unspecified_video_types_with_data<T, const N: usize>(
        &self,
        supported_video_types_and_data: [(VideoType, T); N],
    ) -> Option<Vec<(VideoType, T)>> {
        let supported_video_types = supported_video_types_and_data
            .iter()
            .map(|(video_type, _)| *video_type)
            .collect::<Vec<_>>();
        let selected_video_types = self.convert_to_non_unspecified_video_types(&supported_video_types);
        let selected_video_types_and_data = supported_video_types_and_data
            .into_iter()
            .filter(|(video_type, _)| selected_video_types.contains(&video_type))
            .collect::<Vec<_>>();

        if selected_video_types_and_data.is_empty() {
            None
        } else {
            Some(selected_video_types_and_data)
        }
    }
}

impl Display for VideoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoType::Unspecified(Language::Unspecified) => write!(f, "Unspecified"),
            VideoType::Unspecified(language) => write!(f, "{}", language.get_name_long()),
            VideoType::Raw => write!(f, "Raw"),
            VideoType::Dub(Language::Unspecified) => write!(f, "Dub"),
            VideoType::Sub(Language::Unspecified) => write!(f, "Sub"),
            VideoType::Dub(language) => write!(f, "{}Dub", language.get_name_short()),
            VideoType::Sub(language) => write!(f, "{}Sub", language.get_name_short()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Sequence)]
pub enum Language {
    #[clap(hide = true)]
    Unspecified,
    #[clap(aliases = ["eng"])]
    English,
    #[clap(aliases = ["ger"])]
    German,
}

impl Language {
    pub fn get_name_short(&self) -> &'static str {
        match self {
            Language::Unspecified => "Und",
            Language::English => "Eng",
            Language::German => "Ger",
        }
    }

    pub fn get_name_long(&self) -> &'static str {
        match self {
            Language::Unspecified => "Unspecified",
            Language::English => "English",
            Language::German => "German",
        }
    }
}

impl<'a> TryFrom<&'a str> for Language {
    type Error = anyhow::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let language = match value.to_ascii_lowercase().deref() {
            "english" | "eng" => Language::English,
            "german" | "ger" => Language::German,
            _ => {
                anyhow::bail!("could not recognize language: {}", value);
            }
        };

        Ok(language)
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub language: VideoType,
    pub episodes: EpisodesRequest,
    pub extractor_priorities: Vec<ExtractorMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpisodesRequest {
    Unspecified,
    All,
    Episodes(AllOrSpecific),
    Seasons(AllOrSpecific),
    Combined { seasons: AllOrSpecific, episodes: AllOrSpecific },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllOrSpecific {
    All,
    Specific(Vec<RangeInclusive<u32>>),
}

impl AllOrSpecific {
    pub fn contains(&self, number: u32) -> bool {
        match self {
            AllOrSpecific::All => true,
            AllOrSpecific::Specific(ranges) => ranges.iter().any(|range| range.contains(&number)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadSettings<F: FnMut() -> Duration> {
    pub ddos_wait_episodes: Option<NonZeroU32>,
    pub ddos_wait_time: F,
    counter: u32,
}

impl<F: FnMut() -> Duration> DownloadSettings<F> {
    pub fn new(ddos_wait_episodes: Option<NonZeroU32>, ddos_wait_time: F) -> Self {
        Self {
            ddos_wait_episodes,
            ddos_wait_time,
            counter: 0,
        }
    }

    async fn maybe_ddos_wait(&mut self) {
        if let Some(counter_match) = &self.ddos_wait_episodes {
            self.counter += 1;

            if self.counter == counter_match.get() {
                self.counter = 0;
                tokio::time::sleep((self.ddos_wait_time)()).await;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractorMatch {
    Name(String),
    Any,
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub episode_info: EpisodeInfo,
    pub language: VideoType,
    pub download_url: String,
    pub referer: Option<String>,
}

impl DownloadTask {
    pub fn new(episode_info: EpisodeInfo, language: VideoType, extracted_video: ExtractedVideo) -> Self {
        Self {
            episode_info,
            language,
            download_url: extracted_video.url,
            referer: extracted_video.referer,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EpisodeInfo {
    pub name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: EpisodeNumber,
    pub max_episode_number_in_season: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpisodeNumber {
    Number(u32),
    String(String),
}

#[enum_dispatch]
pub trait InstantiatedDownloader {
    async fn get_series_info(&self) -> Result<SeriesInfo, anyhow::Error>;

    async fn download<F: FnMut() -> Duration>(
        &self,
        request: DownloadRequest,
        settings: DownloadSettings<F>,
        sender: UnboundedSender<DownloadTask>,
    ) -> Result<(), anyhow::Error>;
}

pub trait Downloader<'driver>: InstantiatedDownloader {
    fn new(driver: &'driver thirtyfour::WebDriver, browser_visible: bool, url: String) -> Self;

    async fn supports_url(url: &str) -> bool;
}

pub mod utils {
    use std::time::Duration;

    use rand::distributions::uniform::SampleRange;
    use rand::Rng;

    pub async fn sleep_random<R: SampleRange<u64>>(ms_range: R) {
        if ms_range.is_empty() {
            return;
        }

        let mut rng = rand::thread_rng();
        let duration = rng.gen_range(ms_range);
        tokio::time::sleep(Duration::from_millis(duration)).await;
    }

    pub async fn sleep_jitter(ms_sleep: u64, ms_jitter: u64) {
        let min = ms_sleep.saturating_sub(ms_jitter);
        let max = ms_sleep.saturating_add(ms_jitter);

        if min == max && min != 0 {
            tokio::time::sleep(Duration::from_millis(min)).await;
        } else {
            sleep_random(min..=max).await
        }
    }
}
