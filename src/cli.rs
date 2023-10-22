use std::fmt::Display;
use std::num::NonZeroU32;
use std::ops::RangeInclusive;

use clap::{Parser, ValueEnum};

use crate::downloaders::{EpisodesRequest, Language, VideoType};

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
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::All, hide_default_value = true)]
    pub(crate) episodes: SimpleRanges,

    /// Only download specific seasons
    #[arg(short, long, value_parser = parse_ranges, default_value_t = SimpleRanges::All, hide_default_value = true, conflicts_with_all = ["episodes"])]
    pub(crate) seasons: SimpleRanges,

    /// Use underlying extractors directly
    #[arg(short = 'u', long, conflicts_with_all = ["video_type", "language", "episodes", "seasons"])]
    pub(crate) extractor: bool,

    /// Concurrent downloads
    #[arg(short = 'N', long, default_value = "5")]
    pub(crate) concurrent_downloads: Option<NonZeroU32>,

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
            (SimpleRanges::All, SimpleRanges::All) => EpisodesRequest::All,
            (SimpleRanges::Custom(episodes), SimpleRanges::All) => EpisodesRequest::Episodes(episodes),
            (SimpleRanges::All, SimpleRanges::Custom(seasons)) => EpisodesRequest::Seasons(seasons),
            (SimpleRanges::Custom(_), SimpleRanges::Custom(_)) => unreachable!(),
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
    All,
    Custom(Vec<RangeInclusive<u32>>),
}

impl Display for SimpleRanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimpleRanges::All => write!(f, "All"),
            SimpleRanges::Custom(_) => write!(f, "Custom"),
        }
    }
}

fn parse_ranges(input: &str) -> Result<SimpleRanges, String> {
    const BEFORE_LAST: u32 = u32::MAX - 1;

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

            if begin < 1 {
                return Err(format!("range has to start with at least 1: \"{part}\""));
            }

            if begin > end {
                return Err(format!("range start cannot be bigger than range end: \"{part}\""));
            }

            ranges.push(begin..=end);
        } else {
            let Ok(episode @ ..=BEFORE_LAST) = part.parse::<u32>() else {
                return Err(format!("failed to parse \"{part}\" as integer"));
            };

            if episode < 1 {
                return Err(format!("episode number has to be at least 1: \"{part}\""));
            }

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
