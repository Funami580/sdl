use std::fmt::Display;
use std::num::NonZeroU32;
use std::ops::RangeInclusive;
use std::str::FromStr;
use std::time::Duration;

use clap::{Parser, ValueEnum};

use crate::downloaders::{AllOrSpecific, DownloadSettings, EpisodesRequest, Language, VideoType};

#[derive(Parser, Debug)]
#[command(version)]
/// Download multiple episodes from streaming sites
pub(crate) struct Args {
    /// Only download specific type
    #[arg(value_enum, short = 't', long = "type", ignore_case = true, default_value_t = SimpleVideoType::Unspecified, hide_default_value = true)]
    pub(crate) video_type: SimpleVideoType,

    /// Only download specific language
    #[arg(value_enum, short, long, ignore_case = true, default_value_t = Language::Unspecified, hide_default_value = true)]
    pub(crate) language: Language,

    /// Only download specific episodes
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::Unspecified, hide_default_value = true)]
    pub(crate) episodes: SimpleRanges,

    /// Only download specific seasons
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::Unspecified, hide_default_value = true, conflicts_with_all = ["episodes"])]
    pub(crate) seasons: SimpleRanges,

    /// Use underlying extractors directly
    #[arg(short = 'u', long, num_args = 0..=1, require_equals = true, value_parser = parse_extractor, default_missing_value = "auto", conflicts_with_all = ["video_type", "language", "episodes", "seasons", "concurrent_downloads", "ddos_wait_episodes", "ddos_wait_ms"])]
    pub(crate) extractor: Option<Extractor>,

    /// Concurrent downloads
    #[arg(short = 'N', long, value_parser = parse_optional_with_inf_as_none::<NonZeroU32>, default_value = "5")]
    pub(crate) concurrent_downloads: OptionWrapper<NonZeroU32>,

    /// Amount of episodes to extract before waiting
    #[arg(long, value_parser = parse_optional_with_never_as_none::<NonZeroU32>, default_value = "4")]
    pub(crate) ddos_wait_episodes: OptionWrapper<NonZeroU32>,

    /// The duration in milliseconds to wait
    #[arg(long, default_value_t = 60 * 1000)]
    pub(crate) ddos_wait_ms: u32,

    // Enable debug mode
    #[arg(short, long)]
    pub(crate) debug: bool,

    /// Download URL
    pub(crate) url: String,
}

impl Args {
    pub(crate) fn get_video_type(&self) -> VideoType {
        match self.video_type {
            SimpleVideoType::Unspecified => VideoType::Unspecified,
            SimpleVideoType::Raw => VideoType::Raw,
            SimpleVideoType::Dub => VideoType::Dub(self.language),
            SimpleVideoType::Sub => VideoType::Sub(self.language),
        }
    }

    pub(crate) fn get_episodes_request(self) -> EpisodesRequest {
        match (self.episodes, self.seasons) {
            (SimpleRanges::Unspecified, SimpleRanges::Unspecified) => EpisodesRequest::Unspecified,
            (SimpleRanges::Custom(episodes), SimpleRanges::Unspecified) => {
                EpisodesRequest::Episodes(AllOrSpecific::Specific(episodes))
            }
            (SimpleRanges::Unspecified, SimpleRanges::Custom(seasons)) => {
                EpisodesRequest::Seasons(AllOrSpecific::Specific(seasons))
            }
            (SimpleRanges::All, SimpleRanges::Unspecified) => EpisodesRequest::Episodes(AllOrSpecific::All),
            (SimpleRanges::Unspecified, SimpleRanges::All) => EpisodesRequest::Seasons(AllOrSpecific::All),
            (SimpleRanges::All, SimpleRanges::All) | (SimpleRanges::Custom(_), SimpleRanges::Custom(_)) => {
                unreachable!()
            }
            (SimpleRanges::All, SimpleRanges::Custom(_)) | (SimpleRanges::Custom(_), SimpleRanges::All) => {
                unreachable!()
            }
        }
    }

    pub(crate) fn get_download_settings(&self) -> DownloadSettings<impl FnMut() -> Duration> {
        let wait_duration = Duration::from_millis(self.ddos_wait_ms as u64);
        let wait_fn = move || wait_duration;

        DownloadSettings {
            ddos_wait_episodes: self.ddos_wait_episodes.inner().copied(),
            ddos_wait_time: wait_fn,
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub(crate) enum SimpleVideoType {
    #[clap(hide = true)]
    Unspecified,
    Raw,
    Dub,
    Sub,
}

#[derive(Debug, Clone)]
pub(crate) enum SimpleRanges {
    Unspecified,
    All,
    Custom(Vec<RangeInclusive<u32>>),
}

impl Display for SimpleRanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimpleRanges::Unspecified => write!(f, "Unspecified"),
            SimpleRanges::All => write!(f, "All"),
            SimpleRanges::Custom(_) => write!(f, "Custom"),
        }
    }
}

fn parse_ranges(input: &str) -> Result<SimpleRanges, String> {
    const BEFORE_LAST: u32 = u32::MAX - 1;

    if input.eq_ignore_ascii_case("unspecified") {
        return Ok(SimpleRanges::Unspecified);
    }

    if input.eq_ignore_ascii_case("all") {
        return Ok(SimpleRanges::All);
    }

    let no_space = input.replace(' ', "");
    let parts = no_space.split(',');
    let mut ranges: Vec<RangeInclusive<u32>> = Vec::new();

    for part in parts {
        if let Some((begin, end)) = part.split_once('-') {
            let Ok(begin @ ..=BEFORE_LAST) = begin.parse::<u32>() else {
                return Err(format!("failed to parse \"{begin}\" as integer in range \"{part}\""));
            };

            let Ok(end @ ..=BEFORE_LAST) = end.parse::<u32>() else {
                return Err(format!("failed to parse \"{end}\" as integer in range \"{part}\""));
            };

            if begin > end {
                return Err(format!("range start cannot be bigger than range end: \"{part}\""));
            }

            ranges.push(begin..=end);
        } else {
            let Ok(episode @ ..=BEFORE_LAST) = part.parse::<u32>() else {
                return Err(format!("failed to parse \"{part}\" as integer"));
            };

            ranges.push(episode..=episode);
        }
    }

    let mut lapper = rust_lapper::Lapper::new(
        ranges
            .iter()
            .map(|range| rust_lapper::Interval {
                start: *range.start(),
                stop: *range.end() + 1,
                val: (),
            })
            .collect(),
    );
    lapper.merge_overlaps();
    let merged_ranges = lapper
        .intervals
        .into_iter()
        .map(|interval| interval.start..=(interval.stop - 1))
        .collect();

    Ok(SimpleRanges::Custom(merged_ranges))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Extractor {
    Auto,
    Name(String),
}

fn parse_extractor(input: &str) -> Result<Extractor, String> {
    if input.eq_ignore_ascii_case("auto") {
        Ok(Extractor::Auto)
    } else {
        Ok(Extractor::Name(input.to_owned()))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OptionWrapper<T>(Option<T>);

impl<T> OptionWrapper<T> {
    pub(crate) fn inner(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

fn parse_optional_with_none<T: FromStr>(input: &str, none_value: &'static str) -> Result<OptionWrapper<T>, String>
where
    T::Err: Display,
{
    if input.eq_ignore_ascii_case(none_value) {
        Ok(OptionWrapper(None))
    } else {
        input
            .parse::<T>()
            .map(|value| OptionWrapper(Some(value)))
            .map_err(|err| format!("{err}"))
    }
}

fn parse_optional_with_inf_as_none<T: FromStr>(input: &str) -> Result<OptionWrapper<T>, String>
where
    T::Err: Display,
{
    parse_optional_with_none(input, "inf")
}

fn parse_optional_with_never_as_none<T: FromStr>(input: &str) -> Result<OptionWrapper<T>, String>
where
    T::Err: Display,
{
    parse_optional_with_none(input, "never")
}
