use std::time::Duration;

use anyhow::Context;
use once_cell::sync::Lazy;
use regex::Regex;
use thirtyfour::prelude::ElementQueryable;
use thirtyfour::{By, WebDriver, WebElement};
use tokio::sync::mpsc::UnboundedSender;

use super::{
    AllOrSpecific, DownloadRequest, DownloadSettings, DownloadTask, EpisodeInfo, EpisodeNumber, EpisodesRequest,
    InstantiatedDownloader, Language, SeriesInfo, VideoType,
};
use crate::downloaders::utils::sleep_random;
use crate::downloaders::Downloader;
use crate::extractors::{
    exists_extractor_with_name, extract_video_url_with_extractor_from_source,
    extract_video_url_with_extractor_from_url_unchecked, extractor_supports_source,
};

static URL_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)^https?://(?:www\.)?aniwave\.to/watch/([^/\s]+)(?:/ep-([^/\s]+))?$"#).unwrap());

pub struct Aniwave<'driver> {
    driver: &'driver thirtyfour::WebDriver,
    browser_visible: bool,
    parsed_url: ParsedUrl,
}

impl<'driver> Downloader<'driver> for Aniwave<'driver> {
    fn new(driver: &'driver thirtyfour::WebDriver, browser_visible: bool, url: String) -> Self {
        let parsed_url = ParsedUrl::try_from(&*url).unwrap();
        Self {
            driver,
            browser_visible,
            parsed_url,
        }
    }

    async fn supports_url(url: &str) -> bool {
        ParsedUrl::try_from(url).is_ok()
    }
}

impl InstantiatedDownloader for Aniwave<'_> {
    async fn get_series_info(&self) -> Result<super::SeriesInfo, anyhow::Error> {
        self.driver.goto(self.parsed_url.get_anime_url()).await?;

        if self
            .driver
            .source()
            .await
            .with_context(|| "failed to get page source")?
            .contains("/waf-captcha-verify")
        {
            if self.browser_visible {
                log::warn!("Captcha detected. Please solve the captcha within 30 seconds");
                tokio::time::sleep(Duration::from_secs(30)).await;

                if self
                    .driver
                    .source()
                    .await
                    .with_context(|| "failed to get page source")?
                    .contains("/waf-captcha-verify")
                {
                    anyhow::bail!("captcha detected");
                }
            } else {
                log::error!("Captcha detected. Restart sdl with --debug and solve the captcha");
                anyhow::bail!("captcha detected");
            }
        }

        let title = self
            .driver
            .find(By::Css("h1.title"))
            .await
            .with_context(|| "failed to find title")?
            .text()
            .await
            .with_context(|| "failed to get title")?
            .trim()
            .to_owned();

        let description = self
            .driver
            .execute(
                r#"return document.querySelector(".synopsis .content").innerText;"#,
                vec![],
            )
            .await
            .ok()
            .and_then(|script_ret| {
                let trimmed_desc = script_ret.json().as_str()?.trim();

                if trimmed_desc.is_empty() {
                    None
                } else {
                    Some(trimmed_desc.to_owned())
                }
            });

        Ok(SeriesInfo {
            title,
            description,
            status: None, // too lazy but possible
            year: None,   // too lazy but possible
        })
    }

    async fn download<F: FnMut() -> Duration>(
        &self,
        request: super::DownloadRequest,
        settings: DownloadSettings<F>,
        sender: tokio::sync::mpsc::UnboundedSender<super::DownloadTask>,
    ) -> Result<(), anyhow::Error> {
        let mut scraper = Scraper::new(self.driver, &self.parsed_url, request, settings, sender)?;
        scraper.scrape().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrl {
    anime_id: String,
    episode_id: Option<String>,
}

impl TryFrom<&str> for ParsedUrl {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let captures = URL_REGEX.captures(value).with_context(|| "failed to find captures")?;
        let groups = captures
            .iter()
            .skip(1)
            .filter_map(|x| x.map(|y| y.as_str()))
            .collect::<Vec<_>>();

        let Some(anime_id) = groups.get(0) else {
            anyhow::bail!("failed to find anime id in url");
        };

        let episode_id = groups.get(1);

        Ok(Self {
            anime_id: anime_id.to_string(),
            episode_id: episode_id.map(|inner| inner.to_string()),
        })
    }
}

impl ParsedUrl {
    fn get_anime_url(&self) -> String {
        format!("https://aniwave.to/watch/{}", self.anime_id)
    }

    fn get_episode_url(&self, episode_id: &str) -> String {
        format!("{}/ep-{}", self.get_anime_url(), episode_id)
    }
}

struct Scraper<'driver, 'url, F: FnMut() -> Duration> {
    driver: &'driver WebDriver,
    parsed_url: &'url ParsedUrl,
    request: DownloadRequest,
    settings: DownloadSettings<F>,
    sender: UnboundedSender<DownloadTask>,
    server_selectors: Vec<(VideoType, By)>,
}

impl<'driver, 'url, F: FnMut() -> Duration> Scraper<'driver, 'url, F> {
    fn new(
        driver: &'driver WebDriver,
        parsed_url: &'url ParsedUrl,
        request: DownloadRequest,
        settings: DownloadSettings<F>,
        sender: UnboundedSender<DownloadTask>,
    ) -> Result<Self, anyhow::Error> {
        let language_selectors = Self::get_server_selectors(&request.language)
            .with_context(|| format!("Selected language is not supported for this site: {}", request.language))?;

        Ok(Self {
            driver,
            parsed_url,
            request,
            settings,
            sender,
            server_selectors: language_selectors,
        })
    }

    async fn scrape(&mut self) -> Result<(), anyhow::Error> {
        let episodes_request = std::mem::replace(&mut self.request.episodes, EpisodesRequest::Unspecified);

        match episodes_request {
            EpisodesRequest::Unspecified => {
                if let Some(episode_id) = &self.parsed_url.episode_id {
                    self.scrape_single_episode(episode_id).await
                } else {
                    self.scrape_season(&AllOrSpecific::All).await
                }
            }
            EpisodesRequest::Episodes(episodes) => self.scrape_season(&episodes).await,
            EpisodesRequest::Seasons(_) => {
                anyhow::bail!("AniWave does not support explicit seasons");
            }
        }
    }

    async fn scrape_season(&mut self, episodes: &AllOrSpecific) -> Result<(), anyhow::Error> {
        let anime_url = self.parsed_url.get_anime_url();

        if !self
            .driver
            .current_url()
            .await
            .map(|current_url| current_url.as_str() == anime_url)
            .unwrap_or(false)
        {
            self.driver
                .goto(anime_url)
                .await
                .with_context(|| "failed to go to anime page")?;
            sleep_random(2000..=3000).await; // wait until page has loaded
        }

        let episodes_to_download = self.get_episodes_to_download(episodes).await?;
        let mut got_error = false;

        for episode in episodes_to_download {
            let is_active = episode
                .class_name()
                .await
                .ok()
                .flatten()
                .map(|class_name| class_name.contains("active"))
                .unwrap_or(false);

            if !is_active {
                self.driver
                    .execute(
                        r#"document.querySelectorAll(".episodes ul").forEach(elem => elem.style = "");"#,
                        vec![],
                    )
                    .await
                    .with_context(|| "failed to make all episode buttons visible")?;

                if let Err(err) = episode.click().await {
                    log::warn!("Failed to click on next episode: {}", err);
                    got_error = true;
                    continue;
                }

                sleep_random(2000..=3000).await;
            }

            if let Err(err) = self.send_stream_to_downloader().await {
                log::warn!("Failed to download episode: {}", err);
                got_error = true;
            }

            self.settings.maybe_ddos_wait().await;
        }

        if got_error {
            anyhow::bail!("failed to download all episodes");
        }

        Ok(())
    }

    async fn scrape_single_episode(&self, episode_id: &str) -> Result<(), anyhow::Error> {
        let episode_url = self.parsed_url.get_episode_url(episode_id);

        if !self
            .driver
            .current_url()
            .await
            .map(|current_url| current_url.as_str() == episode_url)
            .unwrap_or(false)
        {
            self.driver
                .goto(self.parsed_url.get_episode_url(episode_id))
                .await
                .with_context(|| "failed to go to episode page")?;
            sleep_random(2000..=3000).await; // wait until page has loaded
        }

        self.send_stream_to_downloader().await
    }

    async fn get_episodes_to_download(
        &self,
        episodes_request: &AllOrSpecific,
    ) -> Result<Vec<WebElement>, anyhow::Error> {
        let episodes = self
            .driver
            .query(By::Css(".episodes a[data-num]"))
            .all_from_selector()
            .await
            .with_context(|| "failed to get all episodes")?;

        if episodes_request == &AllOrSpecific::All {
            return Ok(episodes);
        }

        let mut episodes_with_number = Vec::with_capacity(episodes.len());

        for episode in episodes {
            let number = episode
                .attr("data-num")
                .await
                .ok()
                .flatten()
                .and_then(|number_text| number_text.parse::<u32>().ok());
            episodes_with_number.push((episode, number));
        }

        let mut filtered_episodes = vec![];
        let mut iter = episodes_with_number.into_iter().peekable();

        'episode_loop: while let Some((episode, number_option)) = iter.next() {
            let Some(number) = number_option else {
                continue 'episode_loop;
            };

            if episodes_request.contains(number) {
                filtered_episodes.push(episode);
                continue 'episode_loop;
            }

            if let Some((_, Some(next_number))) = iter.peek() {
                for inter_episode in (number + 1)..*next_number {
                    if episodes_request.contains(inter_episode) {
                        filtered_episodes.push(episode);
                        continue 'episode_loop;
                    }
                }
            }
        }

        Ok(filtered_episodes)
    }

    async fn get_episode_info(&self) -> Result<EpisodeInfo, anyhow::Error> {
        let current_episode = self
            .driver
            .find(By::Css(".episodes a.active"))
            .await
            .with_context(|| "failed to find current episode button")?;
        let current_episode_number_text = current_episode
            .find(By::Tag("b"))
            .await
            .as_ref()
            .unwrap_or(&current_episode)
            .text()
            .await
            .with_context(|| "failed to get current episode number")?;
        let current_episode_number_trimmed = current_episode_number_text.trim();
        let current_episode_number = if let Ok(number) = current_episode_number_trimmed.parse::<u32>() {
            EpisodeNumber::Number(number)
        } else {
            EpisodeNumber::String(current_episode_number_trimmed.to_owned())
        };
        let current_episode_title = if let Ok(title_element) = current_episode.find(By::Tag("span")).await {
            title_element.text().await.ok().and_then(|title| {
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
            .query(By::Css(".episodes a[data-num]"))
            .all_from_selector()
            .await
            .with_context(|| "failed to get all episodes")?;
        let mut max_episode = None;

        for episode in episodes {
            let Some(number_text) = episode.attr("data-num").await.ok().flatten() else {
                log::trace!("Failed to get data-num attribute");
                continue;
            };

            let Ok(number) = number_text.parse::<u32>() else {
                log::trace!("Failed to parse episode as number: {}", number_text);
                continue;
            };

            max_episode = match max_episode {
                Some(old_max) => Some(number.max(old_max)),
                None => Some(number),
            };
        }

        Ok(EpisodeInfo {
            name: current_episode_title,
            season_number: None,
            episode_number: current_episode_number,
            max_episode_number_in_season: max_episode,
        })
    }

    async fn send_stream_to_downloader(&self) -> Result<(), anyhow::Error> {
        let episode_info = self
            .get_episode_info()
            .await
            .with_context(|| "failed to get episode info")?;
        let (video_type, server_elements) = self
            .get_server_elements()
            .await
            .with_context(|| "failed to find episode in requested language")?;

        'server_loop: for server_element in server_elements {
            let Ok(stream_platform_name) = server_element.text().await else {
                log::trace!("Failed to find name of stream platform");
                continue 'server_loop;
            };

            let stream_platform_name = stream_platform_name.trim();

            if !exists_extractor_with_name(stream_platform_name) {
                continue 'server_loop;
            }

            let is_active = server_element
                .class_name()
                .await
                .ok()
                .flatten()
                .map(|class_name| class_name.contains("active"))
                .unwrap_or(false);

            if !is_active {
                server_element
                    .click()
                    .await
                    .with_context(|| "failed to click server element")?;
                sleep_random(2000..=3000).await;
            }

            let player_div = self
                .driver
                .find(By::Css("div#player"))
                .await
                .with_context(|| "failed to find player div")?;

            'video_loop: for _ in 0..5 {
                let Ok(video_frame) = self.driver.find(By::Css("div#player > iframe")).await else {
                    player_div.click().await.with_context(|| "failed to click player div")?;
                    sleep_random(2000..=3000).await;
                    continue 'video_loop;
                };

                let extracted_video = if extractor_supports_source(stream_platform_name).unwrap() {
                    video_frame
                        .enter_frame()
                        .await
                        .with_context(|| "failed to enter video frame")?;

                    let Ok(iframe_source) = self.driver.source().await else {
                        log::trace!("Failed to get iframe source");
                        self.driver.enter_parent_frame().await.unwrap();
                        continue 'server_loop;
                    };

                    self.driver.enter_parent_frame().await.unwrap();

                    extract_video_url_with_extractor_from_source(iframe_source, stream_platform_name)
                        .await
                        .unwrap()
                } else {
                    let Ok(Some(iframe_url)) = video_frame.attr("src").await else {
                        log::trace!("Failed to find src attribute of iframe");
                        continue 'server_loop;
                    };

                    extract_video_url_with_extractor_from_url_unchecked(&iframe_url, stream_platform_name, None, None)
                        .await
                        .unwrap()
                };

                match extracted_video {
                    Ok(extracted_video) => {
                        self.sender
                            .send(DownloadTask::new(episode_info, video_type, extracted_video))
                            .unwrap();
                        return Ok(());
                    }
                    Err(err) => {
                        log::trace!("Failed to extract video url from stream: {:#}", err);
                    }
                }

                break 'video_loop;
            }
        }

        anyhow::bail!("failed to get video url for episode")
    }

    fn get_server_selectors(video_type: &VideoType) -> Option<Vec<(VideoType, By)>> {
        let supported_video_types_and_selector = [
            (
                VideoType::Sub(Language::English),
                By::Css(r#"div.servers > div[data-type="sub"] > ul > li"#),
            ),
            (
                VideoType::Dub(Language::English),
                By::Css(r#"div.servers > div[data-type="dub"] > ul > li"#),
            ),
        ];

        video_type.convert_to_non_unspecified_video_types_with_data(supported_video_types_and_selector)
    }

    async fn get_server_elements(&self) -> Option<(VideoType, Vec<WebElement>)> {
        for (video_type, selector) in &self.server_selectors {
            let Ok(servers) = self.driver.query(selector.clone()).all_from_selector_required().await else {
                continue;
            };

            return Some((*video_type, servers));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Aniwave, ParsedUrl};
    use crate::downloaders::Downloader;

    #[tokio::test]
    async fn test_supports_url() {
        let is_supported = [
            "https://aniwave.to/watch/case-closed.myz/ep-316317",
            "https://aniwave.to/watch/case-closed.myz/ep-345-b",
            "https://aniwave.to/watch/case-closed.myz/ep-222-224",
            "https://aniwave.to/watch/case-closed-crossroad-in-the-ancient-capital.mq2x",
            "http://aniwave.to/watch/case-closed-crossroad-in-the-ancient-capital.mq2x",
            "http://www.aniwave.to/watch/case-closed-crossroad-in-the-ancient-capital.mq2x",
            "https://www.aniwave.to/watch/case-closed-crossroad-in-the-ancient-capital.mq2x",
        ];

        for url in is_supported {
            assert!(Aniwave::supports_url(url).await);
        }
    }

    #[test]
    fn test_parsed_url() {
        let url1 = "https://aniwave.to/watch/case-closed.myz/ep-316317";
        let expected1 = ParsedUrl {
            anime_id: "case-closed.myz".to_string(),
            episode_id: Some("316317".to_string()),
        };

        let url2 = "https://aniwave.to/watch/case-closed.myz/ep-345-b";
        let expected2 = ParsedUrl {
            anime_id: "case-closed.myz".to_string(),
            episode_id: Some("345-b".to_string()),
        };

        let url3 = "https://aniwave.to/watch/case-closed.myz/ep-222-224";
        let expected3 = ParsedUrl {
            anime_id: "case-closed.myz".to_string(),
            episode_id: Some("222-224".to_string()),
        };

        let url4 = "https://aniwave.to/watch/case-closed-crossroad-in-the-ancient-capital.mq2x";
        let expected4 = ParsedUrl {
            anime_id: "case-closed-crossroad-in-the-ancient-capital.mq2x".to_string(),
            episode_id: None,
        };

        let tests = [
            (url1, expected1),
            (url2, expected2),
            (url3, expected3),
            (url4, expected4),
        ];

        for (input, output) in tests {
            assert_eq!(ParsedUrl::try_from(input).unwrap(), output);
        }
    }
}
