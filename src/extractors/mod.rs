use bitmask_enum::bitmask;

use crate::download;
use crate::extractors::dummy::Dummy;
use crate::extractors::filemoon::Filemoon;
use crate::extractors::speedfiles::Speedfiles;
use crate::extractors::streamtape::Streamtape;
use crate::extractors::vidoza::Vidoza;
use crate::extractors::voe::Voe;

pub mod dummy;
pub mod filemoon;
pub mod speedfiles;
pub mod streamtape;
pub mod vidoza;
pub mod voe;

macro_rules! normalized_name {
    ($extractor:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            Some(<$ext>::DISPLAY_NAME)
        } else {
            normalized_name!($extractor, $($tail),*)
        }
    };
    ($extractor:expr $(,)?) => {
        None
    };
}

macro_rules! exists_extractor_with_name {
    ($extractor:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            true
        } else {
            exists_extractor_with_name!($extractor, $($tail),*)
        }
    };
    ($extractor:expr $(,)?) => {
        false
    };
}

macro_rules! exists_extractor_for_url {
    ($url:expr, $extractor:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if let Some(extractor_name) = $extractor {
            if <$ext>::NAMES.iter().any(|name| extractor_name.eq_ignore_ascii_case(name)) {
                <$ext>::supports_url($url).await.unwrap_or(true)
            } else {
                exists_extractor_for_url!($url, $extractor, $($tail),*)
            }
        } else {
            if <$ext>::supports_url($url).await.unwrap_or(false) {
                true
            } else {
                exists_extractor_for_url!($url, $extractor, $($tail),*)
            }
        }
    };
    ($url:expr, $extractor:expr $(,)?) => {
        false
    };
}

macro_rules! extractor_supports_source {
    ($extractor:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            Some(<$ext>::supported_from().contains(SupportedFrom::Source))
        } else {
            extractor_supports_source!($extractor, $($tail),*)
        }
    };
    ($extractor:expr $(,)?) => {
        None
    };
}

macro_rules! extract_video_url {
    ($url:expr, $user_agent:expr, $referer:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::supports_url($url).await.unwrap_or(false) {
            Some(<$ext>::extract_video_url(ExtractFrom::Url { url: $url.to_owned(), user_agent: $user_agent, referer: $referer }).await)
        } else {
            extract_video_url!($url, $user_agent, $referer, $($tail),*)
        }
    };
    ($url:expr, $user_agent:expr, $referer:expr $(,)?) => {
        None
    };
}

macro_rules! extract_video_url_with_extractor_from_url {
    ($url:expr, $extractor:expr, $user_agent:expr, $referer:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            if <$ext>::supports_url($url).await.unwrap_or(true) {
                Some(<$ext>::extract_video_url(ExtractFrom::Url { url: $url.to_owned(), user_agent: $user_agent, referer: $referer }).await)
            } else {
                None
            }
        } else {
            extract_video_url_with_extractor_from_url!($url, $extractor, $user_agent, $referer, $($tail),*)
        }
    };
    ($url:expr, $extractor:expr, $user_agent:expr, $referer:expr $(,)?) => {
        None
    };
}

macro_rules! extract_video_url_with_extractor_from_url_unchecked {
    ($url:expr, $extractor:expr, $user_agent:expr, $referer:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            Some(<$ext>::extract_video_url(ExtractFrom::Url { url: $url.to_owned(), user_agent: $user_agent, referer: $referer }).await)
        } else {
            extract_video_url_with_extractor_from_url_unchecked!($url, $extractor, $user_agent, $referer, $($tail),*)
        }
    };
    ($url:expr, $extractor:expr, $user_agent:expr, $referer:expr $(,)?) => {
        None
    };
}

macro_rules! extract_video_url_with_extractor_from_source {
    ($source:expr, $extractor:expr, $ext:ty $(, $tail:ty)* $(,)?) => {
        if <$ext>::NAMES.iter().any(|name| $extractor.eq_ignore_ascii_case(name)) {
            Some(<$ext>::extract_video_url(ExtractFrom::Source($source)).await)
        } else {
            extract_video_url_with_extractor_from_source!($source, $extractor, $($tail),*)
        }
    };
    ($source:expr, $extractor:expr $(,)?) => {
        None
    };
}

macro_rules! create_functions_for_extractors {
    ($( $ext:ty ),* $(,)?) => {
        pub fn normalized_name(extractor: &str) -> Option<&'static str> {
            normalized_name!(extractor, $($ext),*)
        }

        pub fn exists_extractor_with_name(extractor: &str) -> bool {
            exists_extractor_with_name!(extractor, $($ext),*)
        }

        pub async fn exists_extractor_for_url(url: &str, extractor: Option<&str>) -> bool {
            exists_extractor_for_url!(url, extractor, $($ext),*)
        }

        pub fn extractor_supports_source(extractor: &str) -> Option<bool> {
            extractor_supports_source!(extractor, $($ext),*)
        }

        pub async fn extract_video_url(url: &str, user_agent: Option<String>, referer: Option<String>) -> Option<Result<ExtractedVideo, anyhow::Error>> {
            extract_video_url!(url, user_agent, referer, $($ext),*)
        }

        pub async fn extract_video_url_with_extractor_from_url(url: &str, extractor: &str, user_agent: Option<String>, referer: Option<String>) -> Option<Result<ExtractedVideo, anyhow::Error>> {
            extract_video_url_with_extractor_from_url!(url, extractor, user_agent, referer, $($ext),*)
        }

        pub async fn extract_video_url_with_extractor_from_url_unchecked(url: &str, extractor: &str, user_agent: Option<String>, referer: Option<String>) -> Option<Result<ExtractedVideo, anyhow::Error>> {
            extract_video_url_with_extractor_from_url_unchecked!(url, extractor, user_agent, referer, $($ext),*)
        }

        pub async fn extract_video_url_with_extractor_from_source(source: String, extractor: &str) -> Option<Result<ExtractedVideo, anyhow::Error>> {
            extract_video_url_with_extractor_from_source!(source, extractor, $($ext),*)
        }

        const _CHECK_UNIQUE_NAMES: () = {
            let names_array = [$(<$ext>::NAMES),*];

            let mut i = 0;
            while i < names_array.len() {
                let names = names_array[i];
                i += 1;

                let mut j = 0;
                while j < names.len() {
                    let name = names[j];
                    j += 1;

                    let mut count = 0;
                    let mut i2 = 0;
                    while i2 < names_array.len() {
                        let names2 = names_array[i2];
                        i2 += 1;

                        let mut j2 = 0;
                        while j2 < names2.len() {
                            let name2 = names2[j2];
                            j2 += 1;

                            if name.len() == name2.len() {
                                let mut equal = true;
                                let bytes1 = name.as_bytes();
                                let bytes2 = name2.as_bytes();
                                let mut k = 0;
                                while k < bytes1.len() {
                                    let b1 = bytes1[k];
                                    let b2 = bytes2[k];
                                    k += 1;
                                    if !b1.eq_ignore_ascii_case(&b2) {
                                        equal = false;
                                        break;
                                    }
                                }
                                if equal {
                                    count += 1;
                                }
                            }
                        }
                    }

                    if count != 1 {
                        panic!("Non-unique extractor name!");
                    }
                }
            }
        };
    };
    () => {};
}

create_functions_for_extractors! {
    Dummy,
    Filemoon,
    Speedfiles,
    Streamtape,
    Vidoza,
    Voe,
}

#[derive(Debug, Clone)]
pub enum ExtractFrom {
    Url {
        url: String,
        user_agent: Option<String>,
        referer: Option<String>,
    },
    Source(String),
}

impl ExtractFrom {
    pub async fn get_source(self, referer: Option<&str>) -> Result<String, anyhow::Error> {
        match self {
            ExtractFrom::Url {
                url,
                user_agent,
                referer: referer_input,
            } => download::get_page_text(url, user_agent.as_deref(), referer_input.as_deref().or(referer), None).await,
            ExtractFrom::Source(source) => Ok(source),
        }
    }
}

#[bitmask]
pub enum SupportedFrom {
    Url,
    Source,
}

#[derive(Debug, Clone)]
pub struct ExtractedVideo {
    pub url: String,
    pub referer: Option<String>,
}

pub trait Extractor {
    const DISPLAY_NAME: &'static str;
    const NAMES: &'static [&'static str];

    fn supported_from() -> SupportedFrom;

    async fn supports_url(url: &str) -> Option<bool>;

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error>;
}

pub mod utils {
    use std::collections::{HashMap, VecDeque};
    use std::num::NonZeroUsize;

    use once_cell::sync::Lazy;
    use regex::Regex;

    pub fn is_url_host_and_has_path(url: &str, host: &str, allow_http: bool, allow_www: bool) -> bool {
        url::Url::parse(url)
            .map(|url| {
                let scheme = url.scheme();
                let is_correct_scheme = scheme == "https" || (allow_http && scheme == "http");

                let no_username = url.username().is_empty();
                let no_password = url.password().is_none();
                let no_port = url.port().is_none();

                let is_same_host = url
                    .host_str()
                    .map(|url_host| {
                        let new_url_host = if allow_www {
                            url_host.strip_prefix("www.").unwrap_or(url_host)
                        } else {
                            url_host
                        };

                        host.eq_ignore_ascii_case(new_url_host)
                    })
                    .unwrap_or(false);

                let path = url.path();
                let path_is_empty = path.strip_prefix('/').unwrap_or(path).is_empty();

                is_correct_scheme && no_username && no_password && no_port && is_same_host && !path_is_empty
            })
            .unwrap_or(false)
    }

    /// Port of https://github.com/yt-dlp/yt-dlp/blob/4e38e2ae9d7380015349e6aee59c78bb3938befd/yt_dlp/utils/_utils.py#L4354-L4361
    fn base_n_table(base: NonZeroUsize, table: Option<&'static [u8]>) -> Option<&'static [u8]> {
        const DEFAULT_TABLE: [u8; 62] = [
            b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h',
            b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z',
            b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', b'M', b'N', b'O', b'P', b'Q', b'R',
            b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z',
        ];
        let table = table.unwrap_or(&DEFAULT_TABLE);

        if base.get() > table.len() {
            None
        } else {
            Some(&table[..base.get()])
        }
    }

    /// Port of https://github.com/yt-dlp/yt-dlp/blob/4e38e2ae9d7380015349e6aee59c78bb3938befd/yt_dlp/utils/_utils.py#L4364-L4374
    pub fn encode_base_n(mut num: usize, base: NonZeroUsize, table: Option<&'static [u8]>) -> Option<String> {
        let table = base_n_table(base, table)?;

        if num == 0 {
            return Some(std::str::from_utf8(&[table[0]]).unwrap().to_string());
        }

        let mut result = VecDeque::new();
        let base = table.len();

        while num > 0 {
            result.push_front(table[num % base]);
            num /= base;
        }

        Some(String::from_utf8(result.into()).unwrap())
    }

    /// Port of https://github.com/yt-dlp/yt-dlp/blob/4e38e2ae9d7380015349e6aee59c78bb3938befd/yt_dlp/utils/_utils.py#L4386-L4401
    pub fn decode_packed_codes(code: &str) -> Option<String> {
        static PACKED_CODES_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"}\('(.+)',(\d+),(\d+),'([^']+)'\.split\('\|'\)").unwrap());
        static WORD_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\w+)\b").unwrap());

        let mobj = PACKED_CODES_REGEX.captures(code)?;
        let obfuscated_code = mobj.get(1)?.as_str();
        let base = mobj.get(2)?.as_str().parse::<NonZeroUsize>().ok()?;
        let mut count = mobj.get(3)?.as_str().parse::<usize>().ok()?;
        let symbols = mobj.get(4)?.as_str().split('|').collect::<Vec<_>>();
        let mut symbol_table = HashMap::with_capacity(count);

        while count > 0 {
            count -= 1;
            let base_n_count = encode_base_n(count, base, None)?;
            let symbols_value = symbols.get(count)?;
            let value = if symbols_value.is_empty() {
                base_n_count.clone()
            } else {
                symbols_value.to_string()
            };
            symbol_table.insert(base_n_count, value);
        }

        let mut replace_errored = false;
        let replaced = WORD_REGEX.replace_all(obfuscated_code, |captures: &regex::Captures| {
            match symbol_table.get(captures.get(0).unwrap().as_str()) {
                Some(group) => group,
                None => {
                    replace_errored = true;
                    ""
                }
            }
        });

        if replace_errored {
            return None;
        }

        Some(replaced.to_string())
    }

    pub fn caesar(input: &str, alphabet: &str, shift: i32) -> String {
        let length = alphabet.len();
        let mut output = String::new();

        for c in input.chars() {
            if let Some(position) = alphabet.find(c) {
                let new_position = (position as i64 + shift as i64).rem_euclid(length as i64);
                let new_char = alphabet.as_bytes()[new_position as usize] as char;
                output.push(new_char);
            } else {
                output.push(c);
            }
        }

        output
    }

    pub fn rot47(input: &str) -> String {
        caesar(
            input,
            "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~",
            47,
        )
    }

    #[cfg(test)]
    mod tests {
        use super::decode_packed_codes;
        use crate::extractors::utils::{caesar, rot47};

        #[test]
        fn test_decode_packed_codes() {
            let input = r#"eval(function(p,a,c,k,e,r){e=String;if(!''.replace(/^/,String)){while(c--)r[c]=k[c]||c;k=[function(e){return r[e]}];e=function(){return'\\w+'};c=1};while(c--)if(k[c])p=p.replace(new RegExp('\\b'+e(c)+'\\b','g'),k[c]);return p}('(0(){4 1="5 6 7 8";0 2(3){9(3)}2(1)})();',10,10,'function|b|something|a|var|some|sample|packed|code|alert'.split('|'),0,{}))"#;
            let expected = Some(
                r#"(function(){var b="some sample packed code";function something(a){alert(a)}something(b)})();"#
                    .to_string(),
            );
            assert_eq!(decode_packed_codes(input), expected);
        }

        #[test]
        fn test_caesar() {
            assert_eq!(
                caesar("HELLO WORLD", "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ", 3),
                "KHOOR ZRUOG".to_string()
            );
            assert_eq!(
                caesar("HELLO WORLD", "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ", -3),
                "EBIIL TLOIA".to_string()
            );
            assert_eq!(
                caesar("HELLO WORLD", "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ", 39),
                "KHOOR ZRUOG".to_string()
            );
            assert_eq!(
                caesar("HELLO WORLD", "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ", -39),
                "EBIIL TLOIA".to_string()
            );
        }

        #[test]
        fn test_rot47() {
            assert_eq!(rot47("dCode Rot-47"), r"5r@56 #@E\cf".to_string());
        }
    }
}
