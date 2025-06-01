use std::cmp::Ordering;
use std::time::Duration;

use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use thirtyfour::prelude::ElementQueryable;
use thirtyfour::{By, WebDriver, WebElement};
use tokio::sync::mpsc::UnboundedSender;

use super::{
    AllOrSpecific, DownloadRequest, DownloadSettings, DownloadTask, EpisodeInfo, EpisodeNumber, ExtractorMatch,
    InstantiatedDownloader, Language, SeriesInfo, VideoType,
};
use crate::downloaders::utils::sleep_random;
use crate::downloaders::{Downloader, EpisodesRequest};
use crate::extractors::{extract_video_url_with_extractor_from_url_unchecked, has_extractor_with_name_other_name};

static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)^https?://(?:www\.)?(?:(aniworld)\.to/anime|(s)\.to/serie|(serienstream)\.to/serie)/stream/([^/\s]+)(?:/(?:(?:staffel-([1-9][0-9]*)(?:/(?:episode-([1-9][0-9]*)/?)?)?)|(?:(filme)(?:/(?:film-([1-9][0-9]*)/?)?)?))?)?$"#)
        .unwrap()
});

pub struct AniWorldSerienStream<'driver> {
    driver: &'driver WebDriver,
    parsed_url: ParsedUrl,
}

impl<'driver> Downloader<'driver> for AniWorldSerienStream<'driver> {
    fn new(driver: &'driver WebDriver, _browser_visible: bool, url: String) -> Self {
        let parsed_url = ParsedUrl::try_from(&*url).unwrap();
        Self { driver, parsed_url }
    }

    async fn supports_url(url: &str) -> bool {
        ParsedUrl::try_from(url).is_ok()
    }
}

impl InstantiatedDownloader for AniWorldSerienStream<'_> {
    async fn get_series_info(&self) -> Result<SeriesInfo, anyhow::Error> {
        self.driver.goto(self.parsed_url.get_series_url()).await?;

        let title = self
            .driver
            .execute(
                r#"return document.querySelector(".series-title > h1 > span").innerText;"#,
                vec![],
            )
            .await
            .context("failed to get title")?
            .json()
            .as_str()
            .context("failed to get title as string")?
            .trim()
            .to_owned();

        let description = if let Ok(element) = self.driver.find(By::Css("p[data-full-description]")).await {
            element.attr("data-full-description").await.ok()
        } else {
            None
        }
        .flatten()
        .and_then(|desc| {
            let trimmed_desc = desc.trim();

            if trimmed_desc.is_empty() {
                None
            } else {
                Some(trimmed_desc.to_owned())
            }
        });

        Ok(SeriesInfo {
            title,
            description,
            status: None,
            year: None,
        })
    }

    async fn download<F: FnMut() -> Duration>(
        &self,
        request: DownloadRequest,
        settings: DownloadSettings<F>,
        sender: UnboundedSender<DownloadTask>,
    ) -> Result<(), anyhow::Error> {
        let mut scraper = Scraper::new(self.driver, &self.parsed_url, request, settings, sender)?;
        scraper.scrape().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrl {
    site: Site,
    name: String,
    season: Option<ParsedUrlSeason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrlSeason {
    season: u32,
    episode: Option<u32>,
}

impl TryFrom<&str> for ParsedUrl {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let captures = URL_REGEX.captures(value).context("failed to find captures")?;
        let groups = captures
            .iter()
            .skip(1)
            .filter_map(|x| x.map(|y| y.as_str()))
            .collect::<Vec<_>>();

        let (Some(site), Some(name)) = (groups.get(0), groups.get(1)) else {
            anyhow::bail!("failed to find site and name in url");
        };

        let site = if site.eq_ignore_ascii_case("aniworld") {
            Site::AniWorld
        } else if site.eq_ignore_ascii_case("s") {
            Site::SerienStreamShort
        } else if site.eq_ignore_ascii_case("serienstream") {
            Site::SerienStreamLong
        } else {
            anyhow::bail!("failed to parse site name");
        };

        let parsed_season = if let Some(season) = groups.get(2) {
            let season = if season.eq_ignore_ascii_case("filme") {
                0
            } else {
                season.parse::<u32>().context("failed to parse season as number")?
            };

            let episode = if let Some(episode) = groups.get(3) {
                Some(episode.parse::<u32>().context("failed to parse episode as number")?)
            } else {
                None
            };

            Some(ParsedUrlSeason { season, episode })
        } else {
            None
        };

        Ok(Self {
            site,
            name: name.to_string(),
            season: parsed_season,
        })
    }
}

impl ParsedUrl {
    fn get_series_url(&self) -> String {
        format!("{}/{}", self.site.get_base_url(), self.name)
    }

    fn get_season_url(&self, season: u32) -> String {
        if season == 0 {
            format!("{}/filme", self.get_series_url())
        } else {
            format!("{}/staffel-{}", self.get_series_url(), season)
        }
    }

    fn get_episode_url(&self, season: u32, episode: u32) -> String {
        if season == 0 {
            format!("{}/film-{}", self.get_season_url(season), episode)
        } else {
            format!("{}/episode-{}", self.get_season_url(season), episode)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Site {
    AniWorld,
    SerienStreamShort,
    SerienStreamLong,
}

impl Site {
    fn get_base_url(&self) -> &'static str {
        match self {
            Site::AniWorld => "https://aniworld.to/anime/stream",
            Site::SerienStreamShort => "https://s.to/serie/stream",
            Site::SerienStreamLong => "https://serienstream.to/serie/stream",
        }
    }
}

struct Scraper<'driver, 'url, F: FnMut() -> Duration> {
    driver: &'driver WebDriver,
    parsed_url: &'url ParsedUrl,
    request: DownloadRequest,
    settings: DownloadSettings<F>,
    sender: UnboundedSender<DownloadTask>,
    language_selectors: Vec<(VideoType, By)>,
}

impl<'driver, 'url, F: FnMut() -> Duration> Scraper<'driver, 'url, F> {
    fn new(
        driver: &'driver WebDriver,
        parsed_url: &'url ParsedUrl,
        request: DownloadRequest,
        settings: DownloadSettings<F>,
        sender: UnboundedSender<DownloadTask>,
    ) -> Result<Self, anyhow::Error> {
        let language_selectors = Self::get_language_selectors(&parsed_url.site, &request.language)
            .with_context(|| format!("Selected language is not supported for this site: {}", request.language))?;

        Ok(Self {
            driver,
            parsed_url,
            request,
            settings,
            sender,
            language_selectors,
        })
    }

    async fn scrape(&mut self) -> Result<(), anyhow::Error> {
        let episodes_request = std::mem::replace(&mut self.request.episodes, EpisodesRequest::Unspecified);

        match episodes_request {
            EpisodesRequest::Unspecified => {
                if let Some(season) = &self.parsed_url.season {
                    if let Some(episode) = season.episode {
                        self.scrape_episode(season.season, episode, true).await
                    } else {
                        self.scrape_season(season.season, &AllOrSpecific::All).await
                    }
                } else {
                    self.scrape_seasons(&AllOrSpecific::All).await
                }
            }
            EpisodesRequest::Episodes(episodes) => {
                let season = self.parsed_url.season.as_ref().map(|season| season.season).unwrap_or(1);
                self.scrape_season(season, &episodes).await
            }
            EpisodesRequest::Seasons(seasons) => self.scrape_seasons(&seasons).await,
        }
    }

    async fn scrape_seasons(&mut self, seasons: &AllOrSpecific) -> Result<(), anyhow::Error> {
        let first_episode_url = self.parsed_url.get_episode_url(1, 1);
        self.driver
            .goto(first_episode_url)
            .await
            .context("failed to go to episode page")?;
        sleep_random(1000..=2000).await; // wait until page has loaded
        self.settings.maybe_ddos_wait().await;

        let seasons_info = self.get_seasons_info().await.context("failed to get seasons info")?;
        let season_start = if seasons_info.has_season_zero { 0 } else { 1 };
        let mut got_error = false;

        for season in season_start..=seasons_info.max_season {
            if seasons.contains(season) {
                if let Err(err) = self.scrape_season(season, &AllOrSpecific::All).await {
                    log::warn!("Failed to download S{season:02}: {err:#}");
                    got_error = true;
                }
            }
        }

        if got_error {
            anyhow::bail!("failed to completely download all seasons");
        }

        Ok(())
    }

    async fn scrape_season(&mut self, season: u32, episodes: &AllOrSpecific) -> Result<(), anyhow::Error> {
        let first_episode_url = self.parsed_url.get_episode_url(season, 1);
        let mut already_is_on_page = false;

        if let Ok(current_url) = self.driver.current_url().await {
            if current_url.as_str().eq_ignore_ascii_case(&first_episode_url) {
                already_is_on_page = true;
            }
        }

        if !already_is_on_page {
            self.driver
                .goto(first_episode_url)
                .await
                .context("failed to go to episode page")?;
            sleep_random(1000..=2000).await; // wait until page has loaded
            self.settings.maybe_ddos_wait().await;
        }

        let max_episodes = self
            .get_episode_info(season, 1)
            .await
            .context("failed to get episode info")?
            .max_episode_number_in_season
            .context("failed to get maximum episode number in season")?;

        let mut goto = false;
        let mut got_error = false;

        for episode in 1..=max_episodes {
            if episodes.contains(episode) {
                if let Err(err) = self.scrape_episode(season, episode, goto).await {
                    log::warn!("Failed to get video url for S{season:02}E{episode:03}: {err:#}");
                    got_error = true;
                }
            }

            goto = true;
        }

        if got_error {
            anyhow::bail!("failed to download complete season");
        }

        Ok(())
    }

    async fn scrape_episode(&mut self, season: u32, episode: u32, goto: bool) -> Result<(), anyhow::Error> {
        if goto {
            self.driver
                .goto(self.parsed_url.get_episode_url(season, episode))
                .await
                .context("failed to go to episode page")?;
            sleep_random(1000..=2000).await; // wait until page has loaded
            self.settings.maybe_ddos_wait().await;
        }

        self.send_stream_to_downloader(season, episode).await
    }

    fn get_language_selectors(site: &Site, video_type: &VideoType) -> Option<Vec<(VideoType, By)>> {
        let mut supported_video_types_and_selector = [
            (
                VideoType::Dub(Language::German),
                By::Css(r#"div.changeLanguageBox > img[title="Deutsch"]"#),
            ),
            (
                VideoType::Sub(Language::German),
                By::Css(
                    r#"div.changeLanguageBox > img[title*="Untertitel Deutsch"], div.changeLanguageBox > img[title*="deutschen Untertitel"]"#,
                ),
            ),
            (
                VideoType::Dub(Language::English),
                By::Css(r#"div.changeLanguageBox > img[title="Englisch"]"#),
            ),
            (
                VideoType::Sub(Language::English),
                By::Css(
                    r#"div.changeLanguageBox > img[title*="Untertitel Englisch"], div.changeLanguageBox > img[title*="englischen Untertitel"]"#,
                ),
            ),
        ];

        match site {
            Site::AniWorld => {
                // Anime are preferred as sub over dub, unless it is the native dub
                supported_video_types_and_selector.sort_by(|(type_a, _), (type_b, _)| match (type_a, type_b) {
                    (VideoType::Dub(Language::German), _) => Ordering::Less,
                    (_, VideoType::Dub(Language::German)) => Ordering::Greater,
                    (VideoType::Sub(_), VideoType::Dub(_)) => Ordering::Less,
                    (VideoType::Dub(_), VideoType::Sub(_)) => Ordering::Greater,
                    _ => Ordering::Equal,
                });
            }
            Site::SerienStreamShort | Site::SerienStreamLong => {}
        }

        video_type.convert_to_non_unspecified_video_types_with_data(supported_video_types_and_selector)
    }

    async fn get_language_element(&self) -> Option<(VideoType, WebElement)> {
        for (video_type, selector) in &self.language_selectors {
            let Ok(element) = self.driver.find(selector.clone()).await else {
                continue;
            };

            return Some((*video_type, element));
        }

        None
    }

    async fn get_seasons_info(&self) -> Result<SeasonsInfo, anyhow::Error> {
        let seasons = self
            .driver
            .query(By::Css("#stream > ul:first-of-type > li"))
            .all_from_selector()
            .await
            .unwrap();
        let mut has_movies = false;
        let mut max_season = None;

        for season in seasons {
            let text = season.text().await.unwrap();
            let text = text.trim();

            if text.eq_ignore_ascii_case("Filme") {
                has_movies = true;
                continue;
            }

            let Ok(number) = text.parse::<u32>() else {
                continue;
            };

            max_season = match max_season {
                Some(old_max) => Some(number.max(old_max)),
                None => Some(number),
            };
        }

        if let Some(max_season) = max_season {
            Ok(SeasonsInfo {
                has_season_zero: has_movies,
                max_season,
            })
        } else {
            anyhow::bail!("failed to find max season");
        }
    }

    async fn get_episode_info(&self, current_season: u32, current_episode: u32) -> Option<EpisodeInfo> {
        let episode_title = if let Ok(element) = self.driver.find(By::Css(".episodeGermanTitle")).await {
            element.text().await.ok().and_then(|title| {
                let trimmed = title.trim();

                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            })
        } else {
            None
        };

        let episodes = self
            .driver
            .query(By::Css("li > a[data-episode-id]"))
            .all_from_selector()
            .await
            .unwrap();
        let mut max_episode = None;

        for episode in episodes {
            let number_text = episode.text().await.unwrap();

            let Ok(number) = number_text.parse::<u32>() else {
                log::trace!("Failed to parse episode as number: {}", number_text);
                continue;
            };

            max_episode = match max_episode {
                Some(old_max) => Some(number.max(old_max)),
                None => Some(number),
            };
        }

        Some(EpisodeInfo {
            name: episode_title,
            season_number: Some(current_season),
            episode_number: EpisodeNumber::Number(current_episode),
            max_episode_number_in_season: max_episode,
        })
    }

    async fn send_stream_to_downloader(
        &mut self,
        current_season: u32,
        current_episode: u32,
    ) -> Result<(), anyhow::Error> {
        let episode_info = self
            .get_episode_info(current_season, current_episode)
            .await
            .context("failed to get episode info")?;
        let (video_type, lang_element) = self
            .get_language_element()
            .await
            .context("failed to find episode in requested language")?;

        let lang_key = lang_element
            .attr("data-lang-key")
            .await
            .unwrap()
            .context("failed to find data-lang-key")?;
        let streams_selector = By::Css(&format!(r#".hosterSiteVideo ul li[data-lang-key="{}"]"#, lang_key));
        let available_streams = self.driver.query(streams_selector).all_from_selector().await.unwrap();

        if available_streams.is_empty() {
            anyhow::bail!("no streams in requested language available");
        }

        // Get all available stream platforms with name and url
        let current_url = self.driver.current_url().await.unwrap();
        let mut stream_platform_name_and_redirect_link = Vec::with_capacity(available_streams.len());

        for stream in available_streams {
            let Some(link_target) = stream.attr("data-link-target").await.unwrap() else {
                log::trace!("Failed to find data-link-target");
                continue;
            };

            let Ok(redirect_link) = current_url.join(&link_target) else {
                log::trace!("Failed to parse redirect link: {}", link_target);
                continue;
            };

            let stream_platform_name = self
                .driver
                .execute(
                    &format!(r#"return document.querySelector('.hosterSiteVideo ul li[data-lang-key="{}"][data-link-target="{}"] h4').innerText;"#, lang_key, link_target),
                    vec![],
                )
                .await
                .context("failed to get name of stream platform")?
                .json()
                .as_str().context("failed to get name of stream platform as string")?
                .trim()
                .to_owned();

            stream_platform_name_and_redirect_link.push((stream_platform_name, redirect_link));
        }

        // Order the stream platforms
        let mut ordered_stream_platforms = Vec::with_capacity(stream_platform_name_and_redirect_link.len());

        for extractor in &self.request.extractor_priorities {
            match extractor {
                ExtractorMatch::Name(extractor_name) => {
                    let index = stream_platform_name_and_redirect_link
                        .iter()
                        .position(|x| has_extractor_with_name_other_name(extractor_name, &x.0));
                    if let Some(index) = index {
                        ordered_stream_platforms.push(stream_platform_name_and_redirect_link.remove(index));
                    }
                }
                ExtractorMatch::Any => {
                    ordered_stream_platforms.extend(stream_platform_name_and_redirect_link.into_iter());
                    break;
                }
            }
        }

        // Try to initiate download for each stream platform
        for (stream_platform_name, redirect_link) in ordered_stream_platforms {
            log::trace!("Trying to use '{stream_platform_name}' stream server...");

            let extracted_video = extract_video_url_with_extractor_from_url_unchecked(
                redirect_link.as_str(),
                &stream_platform_name,
                None,
                Some(current_url.as_str().to_owned()),
            )
            .await;

            match extracted_video {
                Some(Ok(extracted_video)) => {
                    self.sender
                        .send(DownloadTask::new(episode_info, video_type, extracted_video))
                        .unwrap();
                    self.settings.maybe_ddos_wait().await;
                    return Ok(());
                }
                Some(Err(err)) => {
                    log::trace!("Failed to extract video url from stream: {:#}", err);
                    self.settings.maybe_ddos_wait().await;
                }
                None => {
                    log::trace!("Failed to find extractor for stream platform: {}", stream_platform_name);
                    continue;
                }
            }
        }

        anyhow::bail!("failed to get video url for episode")
    }
}

#[derive(Debug, Clone)]
struct SeasonsInfo {
    has_season_zero: bool,
    max_season: u32,
}

#[cfg(test)]
mod tests {
    use super::{AniWorldSerienStream, ParsedUrlSeason, Site};
    use crate::downloaders::aniworldserienstream::ParsedUrl;
    use crate::downloaders::Downloader;

    #[tokio::test]
    async fn test_supports_url() {
        let is_supported = [
            "https://aniworld.to/anime/stream/detektiv-conan",
            "https://aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/staffel-1",
            "https://aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/filme",
            "https://aniworld.to/anime/stream/detektiv-conan/staffel-18/episode-2",
            "http://www.aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/filme",
            "https://s.to/serie/stream/detektiv-conan",
            "https://s.to/serie/stream/detektiv-conan/filme",
            "https://s.to/serie/stream/detektiv-conan/staffel-5",
            "https://s.to/serie/stream/detektiv-conan/staffel-1/episode-1",
            "https://serienstream.to/serie/stream/detektiv-conan",
            "https://serienstream.to/serie/stream/detektiv-conan/filme",
            "https://serienstream.to/serie/stream/detektiv-conan/staffel-5",
            "https://serienstream.to/serie/stream/detektiv-conan/staffel-1/episode-1",
        ];

        for url in is_supported {
            assert!(AniWorldSerienStream::supports_url(url).await);
        }
    }

    #[test]
    fn test_parsed_url() {
        let url1 = "https://aniworld.to/anime/stream/detektiv-conan";
        let expected1 = ParsedUrl {
            site: Site::AniWorld,
            name: "detektiv-conan".to_string(),
            season: None,
        };

        let url2 = "https://aniworld.to/anime/stream/mushoku-tensei-jobless-reincarnation/staffel-1";
        let expected2 = ParsedUrl {
            site: Site::AniWorld,
            name: "mushoku-tensei-jobless-reincarnation".to_string(),
            season: Some(ParsedUrlSeason {
                season: 1,
                episode: None,
            }),
        };

        let url3 = "https://s.to/serie/stream/detektiv-conan/staffel-19/episode-20";
        let expected3 = ParsedUrl {
            site: Site::SerienStreamShort,
            name: "detektiv-conan".to_string(),
            season: Some(ParsedUrlSeason {
                season: 19,
                episode: Some(20),
            }),
        };

        let url4 = "https://s.to/serie/stream/detektiv-conan/filme/film-3";
        let expected4 = ParsedUrl {
            site: Site::SerienStreamShort,
            name: "detektiv-conan".to_string(),
            season: Some(ParsedUrlSeason {
                season: 0,
                episode: Some(3),
            }),
        };

        let url5 = "https://serienstream.to/serie/stream/detektiv-conan/staffel-19/episode-20";
        let expected5 = ParsedUrl {
            site: Site::SerienStreamLong,
            name: "detektiv-conan".to_string(),
            season: Some(ParsedUrlSeason {
                season: 19,
                episode: Some(20),
            }),
        };

        let url6 = "https://serienstream.to/serie/stream/detektiv-conan/filme/film-3";
        let expected6 = ParsedUrl {
            site: Site::SerienStreamLong,
            name: "detektiv-conan".to_string(),
            season: Some(ParsedUrlSeason {
                season: 0,
                episode: Some(3),
            }),
        };

        let tests = [
            (url1, expected1),
            (url2, expected2),
            (url3, expected3),
            (url4, expected4),
            (url5, expected5),
            (url6, expected6),
        ];

        for (input, output) in tests {
            assert_eq!(ParsedUrl::try_from(input).unwrap(), output);
            assert_eq!(ParsedUrl::try_from(&*format!("{input}/")).unwrap(), output);
        }
    }
}
