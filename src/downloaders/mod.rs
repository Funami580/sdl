use std::fmt::Display;
use std::num::NonZeroU32;
use std::ops::{Deref, RangeInclusive};
use std::time::Duration;

use clap::ValueEnum;
use enum_dispatch::enum_dispatch;
use tokio::sync::mpsc::UnboundedSender;

use self::aniwave::Aniwave;
use self::aniworldserienstream::AniWorldSerienStream;
use crate::extractors::ExtractedVideo;

pub mod aniwave;
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

macro_rules! find_downloader_for_url {
    ($driver:expr, $url:expr, $dl:ident $(, $tail:ident)* $(,)?) => {
        if <$dl>::supports_url($url).await {
            Some(DispatchDownloader::from(<$dl>::new($driver, $url.to_owned())))
        } else {
            find_downloader_for_url!($driver, $url, $($tail),*)
        }
    };
    ($driver:expr, $url:expr $(,)?) => {
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

        pub async fn find_downloader_for_url<'driver>(
            driver: &'driver mut thirtyfour::WebDriver,
            url: &str,
        ) -> Option<DispatchDownloader<'driver>> {
            find_downloader_for_url!(driver, url, $($dl),*)
        }
    };
    () => {};
}

create_functions_for_extractors! {
    Aniwave,
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
    Unspecified,
    Raw,
    Dub(Language),
    Sub(Language),
}

impl Display for VideoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoType::Unspecified => write!(f, "Unspecified"),
            VideoType::Raw => write!(f, "Raw"),
            VideoType::Dub(Language::Unspecified) => write!(f, "Dub"),
            VideoType::Sub(Language::Unspecified) => write!(f, "Sub"),
            VideoType::Dub(language) => write!(f, "{}Dub", language.get_name_short()),
            VideoType::Sub(language) => write!(f, "{}Sub", language.get_name_short()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    #[clap(hide = true)]
    Unspecified,
    #[clap(aliases = ["en", "eng"])]
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
}

impl<'a> TryFrom<&'a str> for Language {
    type Error = anyhow::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let language = match value.to_ascii_lowercase().deref() {
            "english" | "en" | "eng" => Language::English,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpisodesRequest {
    Unspecified,
    Episodes(AllOrSpecific),
    Seasons(AllOrSpecific),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllOrSpecific {
    All,
    Specific(Vec<RangeInclusive<u32>>),
}

#[derive(Debug, Clone)]
pub struct DownloadSettings<F: FnMut() -> Duration> {
    pub ddos_wait_episodes: Option<NonZeroU32>,
    pub ddos_wait_time: F,
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
        settings: &DownloadSettings<F>,
        sender: UnboundedSender<DownloadTask>,
    ) -> Result<(), anyhow::Error>;
}

pub trait Downloader<'driver>: InstantiatedDownloader {
    fn new(driver: &'driver mut thirtyfour::WebDriver, url: String) -> Self;

    async fn supports_url(url: &str) -> bool;
}
