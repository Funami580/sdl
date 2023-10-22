use once_cell::sync::Lazy;
use regex::Regex;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor};
use crate::extractors::utils::decode_packed_codes;

pub struct Filemoon {}

impl Extractor for Filemoon {
    async fn supports_url(url: &str) -> Option<bool> {
        Some(is_url_host_and_has_path(url, "filemoon.sx", true, true))
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        static SCRIPT_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"(?s)<script\s+[^>]*?data-cfasync="false"[^>]*>(.+?)</script>"#).unwrap());
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"file:"([^"]+)""#).unwrap());

        let source = from.get_source(None).await?;

        for script in SCRIPT_REGEX.captures_iter(&source) {
            let Some(script_content) = script.get(1).map(|group| group.as_str().trim()) else {
                continue;
            };

            if !script_content.starts_with("eval(") {
                continue;
            }

            let Some(unpacked_script) = decode_packed_codes(script_content) else {
                continue;
            };

            let video_url = VIDEO_URL_REGEX
                .captures(&unpacked_script)
                .and_then(|captures| captures.get(1))
                .map(|video_url| video_url.as_str().to_string());

            if let Some(video_url) = video_url {
                return Ok(ExtractedVideo {
                    url: video_url,
                    referer: None,
                });
            }
        }

        anyhow::bail!("Filemoon: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Filemoon;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_filemoon() {
        let url = "https://filemoon.sx/e/ed0p89ndlpl6?t=4xnZDvwlDV0JxQ%3D%3D&autostart=true";
        assert!(Filemoon::supports_url(url).await.unwrap_or(false));

        let source = r#"<script data-cfasync="false" type="text/javascript">eval(function(p,a,c,k,e,d){while(c--)if(k[c])p=p.replace(new RegExp('\\b'+c.toString(a)+'\\b','g'),k[c]);return p}('n 47={9i:{9h:50,9g:"y"},3w:{9f:\'9e://9d.42.1r\',9c:9b,9a:48,99:3,98:50,97:20,96:1,95:p,94:9,93:0.92,91:48,90:p,}};m 1d=8z 1u.1t.2k(47);m 44=0,43=0;1d.k("8y",1s=>17.16("8x",1s.8w,1s.8v));1d.k("8u",(46,2i)=>17.16("8t 8s",2i?`1s ${2i}`:"8r",46.40));1d.k("8q",8(45,2h){g(45==\'8p\')44+=2h;2u 43+=2h});m c=u("1n");c.8o({8n:[{41:"1a://8m.8l.8k.42.1r/8j/8i/8h/8g/8f.8e?t=8d&s=3r&e=8c&f=3s&8b=30&8a=89&88=87"}],86:"1a://3z-3y.1r/85.3x",2d:"1c%",2c:"1c%",84:"83",82:\'81\',80:"7z",l:[{41:"/26?b=7y&11=7x&40=1a://3z-3y.1r/7w.3x",7v:"7u"}],7t:{7s:1,2l:\'#7r\',7q:\'#7p\',7o:"7n",7m:30,7l:1c,},\'7k\':{"7j":"7i"},7h:"7g",7f:"1a://33.32",7e:{},7d:p,7c:[0.25,0.50,0.75,1,1.25,1.5,2],7b:{7a:7,3w:1d.79()}});m 2f,2g,78;m 77=0,76=0;m c=u("1n");m 3v=0,74=0,73=0,v=0;$.72({71:{\'70-6z\':\'3j-6y\'}});c.k(\'6x\',8(x){g(5>0&&x.15>=5&&2g!=1){2g=1;$(\'1p.6w\').6v(\'6u\')}g(x.15>=v+5||x.15<v){v=x.15;2e.6t(\'y\',6s.6r(v),{6q:60*60*24*7})}});c.k(\'1j\',8(x){3v=x.15});c.k(\'3h\',8(x){3u(x)});c.k(\'6p\',8(){$(\'1p.3t\').6o();2e.3i(\'y\')});8 3u(x){$(\'1p.3t\').6n();g(2f)1x;2f=1;1q=0;g(6m.6l===p){1q=1}$.3p(\'/26?b=6k&3k=y&6j=3s-6i-6h-3r-6g&6f=1&6e=&6d=&1q=\'+1q,8(3q){$(\'#6c\').6b(3q)});m v=2e.3p(\'y\');g(6a(v)>0){2r(8(){c.1j(v)},69)}$(\'.6-d-68-67:66("65")\').19(8(e){3o();u().64(0);u().63(p)});8 3o(){m $1o=$("<1p />").3n({15:"62",2d:"1c%",2c:"1c%",61:0,3l:0,3m:5z,5y:"5x(10%, 10%, 10%, 0.4)","5w-5v":"5u"});$("<5t />").3n({2d:"60%",2c:"60%",3m:5s,"5r-3l":"5q"}).5p({\'5o\':\'/?b=5n&3k=y\',\'5m\':\'0\',\'5l\':\'3j\'}).3g($1o);$1o.19(8(){$(5k).3i();u().3h()});$1o.3g($(\'#1n\'))}u().1j(0);}8 5j(){m l=c.1z(3f);17.16(l);g(l.11>1){2o(i=0;i<l.11;i++){g(l[i].1y==3f){17.16(\'!!=\'+i);c.2m(i)}}}}c.k(\'5i\',8(){n 1i=u("1n");n 1b=1i.5h();n 3e=1b.14(".6-1m-w-29");n 2b=3e.3b(p);n 1k=2b.14(".6-w-29");1k.28.3a="39(-1)";1k.38="37 10 36";n 2a=1b.14(".6-1m-w-3d");2a.35.34(2b,2a);1b.14(".6-1m-w-3d").28.1m="5g";n 3c=1b.14(".6-1f-5f");n 1l=3c.14(".6-w-29");n 13=1l.3b(p);13.28.3a="39(-1)";13.38="37 10 36";13.5e.5d("5c");1l.35.34(13,1l.5b);[1k,13].5a((1f)=>{1f.59=()=>{1i.1j(1i.58()+10)}})});8 27(){}c.k(\'57\',8(){27()});c.k(\'56\',8(){27()});u().2z("/2y/26.2x","55 54 53",8(){n 12=23.52(\'a\');12.31(\'51\',\'1a://33.32/4z/y\');12.31(\'4y\',\'4x\');23.1v.4w(12);12.19();23.1v.4v(12)},"4u");c.k("h",8(1h){m l=c.1z();g(l.11<2)1x;$(\'.6-d-4t-4s\').4r(8(){$(\'#6-d-j-h\').1g(\'6-d-j-18\');$(\'.6-j-h\').r(\'o-q\',\'z\')});c.2z("/2y/4q.2x","2t 2s",8(e){$(\'.6-2w\').4p(\'6-d-2v\');g($(\'.6-2w\').4o(\'6-d-2v\')){$(\'.6-d-h\').r(\'o-q\',\'p\');$(\'.6-d-j-h \').r(\'o-q\',\'p\');$(\'.6-d-j-h \').4n(\'6-d-j-18\')}2u{$(\'.6-d-h\').r(\'o-q\',\'z\');$(\'.6-d-j-h \').r(\'o-q\',\'z\');$(\'.6-d-j-h \').1g(\'6-d-j-18\')}$(\'.6-4m .6-w:4l([o-4k="2t 2s"])\').k(\'19\',8(){$(\'.6-d-h\').r(\'o-q\',\'z\');$(\'.6-d-j-h \').r(\'o-q\',\'z\');$(\'.6-d-j-h \').1g(\'6-d-j-18\')})},"4j");c.k("4i",8(1h){22.4h(\'21\',1h.l[1h.4g].1y)});g(22.2q(\'21\')){2r("2p(22.2q(\'21\'));",4f)}});m 1w;8 2p(2n){m l=c.1z();g(l.11>1){2o(i=0;i<l.11;i++){g(l[i].1y==2n){g(i==1w){1x}1w=i;c.2m(i)}}}}$(\'1v\').k(\'19\',\'.6-w-d\',8(){$(\'.6-d-j-h \').1g(\'6-d-j-18\');$(\'.6-1f-2l.6-d-h\').r(\'o-q\',\'z\')});n 2j=4e(()=>{17.16(c.1e);g(c.1e&&c.1e.4d&&1u.1t.2k.4c()){4b(2j);1u.1t.4a(c.1e)}},49);',36,343,'||||||jw||function||||videop|settings|||if|audioTracks||submenu|on|tracks|var|const|aria|true|expanded|attr|||jwplayer|lastt|icon||ed0p89ndlpl6|false||length|dl_item|forwardControlBarButton|querySelector|position|log|console|active|click|https|playerContainer|100|engine|hls|button|removeClass|event|player|seek|forwardDisplayButton|rewindControlBarButton|display|vplayer|dd|div|adb|com|peer|hlsjs|p2pml|body|current_audio|return|name|getAudioTracks||default_audio|localStorage|document|||dl|callMeMaybe|style|rewind|nextContainer|forwardContainer|height|width|ls|vvplay|vvad|size|peerId|iid|Engine|color|setCurrentAudioTrack|audio_name|for|audio_set|getItem|setTimeout|Track|Audio|else|open|controls|svg|images|addButton||setAttribute|sx|filemoon|insertBefore|parentNode|Seconds|Forward|ariaLabel|scaleX|transform|cloneNode|buttonContainer|next|rewindContainer|track_name|appendTo|play|remove|no|file_code|top|zIndex|css|showCCform|get|data|1697939838|24152475|video_ad|doPlay|prevt|loader|jpg|place|img|url|file|cdn112|loaded_p2p|loaded_http|method|segment|p2pconfig|1000|200|initHlsJsPlayer|clearInterval|isSupported|config|setInterval|300|currentTrack|setItem|audioTrackChanged|dualSound|label|not|controlbar|addClass|hasClass|toggleClass|dualy|mousedown|buttons|topbar|download11|removeChild|appendChild|_blank|target|download||href|createElement|Video|This|Download|playAttemptFailed|beforePlay|getPosition|onclick|forEach|nextElementSibling|forward|add|classList|container|none|getContainer|ready|set_audio_track|this|scrolling|frameborder|upload_srt|src|prop|50px|margin|1000001|iframe|center|align|text|rgba|background|1000000||left|absolute|pause|setCurrentCaptions|Upload|contains|item|content|500|parseInt|html|fviews|referer|prem|embed|c884d699a1bd4b2bfc17f583c904e1f6|249|185|hash|view|ZorDon|window|hide|show|complete|ttl|round|Math|set|slow|fadeIn|video_ad_fadein|time|cache|Cache|Content|headers|ajaxSetup|v2done|tott||vastdone2|vastdone1|vvbefore|createLoaderClass|liveSyncDurationCount|hlsjsConfig|playbackRates|playbackRateControls|cast|aboutlink|FileMoon|abouttext|1080p|1415|qualityLabels|fontOpacity|backgroundOpacity|Tahoma|fontFamily|303030|backgroundColor|FFFFFF|userFontScale|captions|thumbnails|kind|ed0p89ndlpl60000|1418|get_slides|start|startparam|auto|preload|uniform|stretching|ed0p89ndlpl6_xt|image|2500|sp|12329|asn|srv|43200|rvm0EjVpGO2BKMaUJjRPEKrxndDmKgV6VrdJ3HnPsp4|m3u8|master|ed0p89ndlpl6_x|04830|01|hls2|waw05|rcr82|be7713|sources|setup|http|piece_bytes_downloaded|HTTP|from|p2p_segment_loaded|segment_loaded|remoteAddress|id|p2p_peer_connect|peer_connect|new|httpDownloadProbabilitySkipIfNoPeers|httpDownloadProbabilityInterval|06|httpDownloadProbability|httpDownloadMaxPriority|httpUseRanges|simultaneousHttpDownloads|simultaneousP2PDownloads|p2pDownloadMaxPriority|requiredSegmentsPriority|cachedSegmentsCount|86400000|cachedSegmentExpiration|metrika|wss|trackerAnnounce|swarmId|forwardSegmentCount|segments'.split('|')))</script>"#;
        let expected = "https://be7713.rcr82.waw05.cdn112.com/hls2/01/04830/ed0p89ndlpl6_x/master.m3u8?t=rvm0EjVpGO2BKMaUJjRPEKrxndDmKgV6VrdJ3HnPsp4&s=1697939838&e=43200&f=24152475&srv=30&asn=12329&sp=2500";

        let extracted = Filemoon::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
