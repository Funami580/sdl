use std::fmt::Display;
use std::num::NonZeroU32;
use std::ops::RangeInclusive;
use std::str::FromStr;
use std::time::Duration;

use clap::{Parser, ValueEnum};

use crate::downloaders::{AllOrSpecific, DownloadSettings, EpisodesRequest, ExtractorMatch, Language, VideoType};
use crate::extractors::exists_extractor_with_name;

#[derive(Parser, Debug)]
#[command(version)]
/// Download multiple episodes from streaming sites
pub(crate) struct Args {
    /// Only download specific video type
    #[arg(value_enum, long = "type", ignore_case = true, default_value_t = SimpleVideoType::Unspecified, hide_default_value = true)]
    pub(crate) video_type: SimpleVideoType,

    /// Only download specific language
    #[arg(value_enum, long = "lang", ignore_case = true, default_value_t = Language::Unspecified, hide_default_value = true)]
    pub(crate) language: Language,

    /// Shorthand for language and video type
    #[arg(short = 't', value_parser = parse_shorthand, default_value_t = VideoType::Unspecified(Language::Unspecified), hide_default_value = true, conflicts_with_all = ["video_type", "language"])]
    pub(crate) type_language: VideoType,

    /// Only download specific episodes
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::Unspecified, hide_default_value = true, value_name = "RANGES")]
    pub(crate) episodes: SimpleRanges,

    /// Only download specific seasons
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::Unspecified, hide_default_value = true, conflicts_with_all = ["episodes"], value_name = "RANGES")]
    pub(crate) seasons: SimpleRanges,

    /// Extractor priorities
    #[arg(short = 'p', long, value_parser = parse_extractor_priorities, value_name = "PRIORITIES", default_value = "*", hide_default_value = true)]
    pub(crate) extractor_priorities: Box<[ExtractorMatch]>,

    /// Use underlying extractors directly
    #[arg(short = 'u', long, num_args = 0..=1, require_equals = true, value_parser = parse_extractor, default_missing_value = "auto", conflicts_with_all = ["video_type", "language", "type_language", "episodes", "seasons", "extractor_priorities", "concurrent_downloads", "ddos_wait_episodes", "ddos_wait_ms"], value_name = "NAME")]
    pub(crate) extractor: Option<Extractor>,

    /// Concurrent downloads
    #[arg(short = 'N', long, value_parser = parse_optional_with_inf_as_none::<NonZeroU32>, default_value = "5", value_name = "INF|NUMBER")]
    pub(crate) concurrent_downloads: OptionWrapper<NonZeroU32>,

    /// Maximum download rate in bytes per second, e.g. 50K or 4.2MiB
    #[arg(short = 'r', long, value_parser = parse_rate_limit_as_f64, value_name = "RATE", default_value = "inf", hide_default_value = true)]
    pub(crate) limit_rate: f64,

    /// Number of download retries
    #[arg(short = 'R', long, value_parser = parse_optional_with_inf_as_none::<NonZeroU32>, default_value = "5", value_name = "INF|NUMBER")]
    pub(crate) retries: OptionWrapper<NonZeroU32>,

    /// Amount of requests before waiting
    #[arg(long, value_parser = parse_optional_with_never_as_none::<NonZeroU32>, default_value = "4", value_name = "NEVER|NUMBER")]
    pub(crate) ddos_wait_episodes: OptionWrapper<NonZeroU32>,

    /// The duration in milliseconds to wait
    #[arg(long, default_value_t = 60 * 1000, value_name = "MILLISECONDS")]
    pub(crate) ddos_wait_ms: u32,

    /// Play in mpv
    #[arg(long, conflicts_with_all = ["concurrent_downloads", "retries", "limit_rate"])]
    pub(crate) mpv: bool,

    /// Enable debug mode
    #[arg(short, long)]
    pub(crate) debug: bool,

    /// Download URL
    pub(crate) url: String,
}

impl Args {
    pub(crate) fn get_video_type(&self) -> VideoType {
        if self.type_language != VideoType::Unspecified(Language::Unspecified) {
            return self.type_language;
        }

        match self.video_type {
            SimpleVideoType::Unspecified => VideoType::Unspecified(self.language),
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

        DownloadSettings::new(self.ddos_wait_episodes.inner().copied(), wait_fn)
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

fn parse_shorthand(input: &str) -> Result<VideoType, String> {
    if input.eq_ignore_ascii_case("Unspecified") {
        return Ok(VideoType::Unspecified(Language::Unspecified));
    }

    if input.eq_ignore_ascii_case("Raw") {
        return Ok(VideoType::Raw);
    }

    if input.eq_ignore_ascii_case("Dub") {
        return Ok(VideoType::Dub(Language::Unspecified));
    }

    if input.eq_ignore_ascii_case("Sub") {
        return Ok(VideoType::Sub(Language::Unspecified));
    }

    let input_lower = input.to_ascii_lowercase();

    if let Some(lang_short) = input_lower.strip_suffix("dub") {
        for lang in enum_iterator::all::<Language>() {
            if lang == Language::Unspecified {
                continue;
            }

            if lang_short.eq_ignore_ascii_case(lang.get_name_short()) {
                return Ok(VideoType::Dub(lang));
            }
        }
    }

    if let Some(lang_short) = input_lower.strip_suffix("sub") {
        for lang in enum_iterator::all::<Language>() {
            if lang == Language::Unspecified {
                continue;
            }

            if lang_short.eq_ignore_ascii_case(lang.get_name_short()) {
                return Ok(VideoType::Sub(lang));
            }
        }
    }

    for lang in enum_iterator::all::<Language>() {
        if lang == Language::Unspecified {
            continue;
        }

        if input.eq_ignore_ascii_case(lang.get_name_long()) {
            return Ok(VideoType::Unspecified(lang));
        }
    }

    for lang in enum_iterator::all::<Language>() {
        if lang == Language::Unspecified {
            continue;
        }

        if input.eq_ignore_ascii_case(lang.get_name_short()) {
            return Ok(VideoType::Unspecified(lang));
        }
    }

    Err(format!("failed to parse \"{input}\" as video type shorthand"))
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

fn parse_extractor_priorities(input: &str) -> Result<Box<[ExtractorMatch]>, String> {
    let no_space = input.replace(' ', "");
    let mut parts = no_space.split(',').peekable();

    let mut out = Vec::new();
    while let Some(part) = parts.next() {
        if part == "*" {
            if parts.peek().is_some() {
                return Err(format!("fallback extractor '*' can only be used at last position"));
            }

            out.push(ExtractorMatch::Any);
            break;
        } else if exists_extractor_with_name(part) {
            out.push(ExtractorMatch::Name(part.to_string()));
        } else {
            return Err(format!("no extractor with name: {part}"));
        }
    }

    Ok(out.into_boxed_slice())
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

fn parse_rate_limit_as_f64(input: &str) -> Result<f64, String> {
    if input.eq_ignore_ascii_case("inf") {
        return Ok(f64::INFINITY);
    }

    let bytes = byte_unit::Byte::parse_str(input, false)
        .map_err(|err| format!("{err}"))?
        .as_u64() as f64;

    if bytes <= 0.0 {
        return Err("rate limit must be greater than 0".to_string());
    }

    Ok(bytes)
}
