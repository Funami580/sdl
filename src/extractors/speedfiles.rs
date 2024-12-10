use anyhow::Context;
use base64::Engine;
use once_cell::sync::Lazy;
use regex::Regex;

use super::utils::is_url_host_and_has_path;
use super::{ExtractFrom, ExtractedVideo, Extractor, SupportedFrom};

pub struct Speedfiles;

impl Extractor for Speedfiles {
    const DISPLAY_NAME: &'static str = "Speedfiles";
    const NAMES: &'static [&'static str] = &["Speedfiles"];

    fn supported_from() -> SupportedFrom {
        SupportedFrom::all()
    }

    async fn supports_url(url: &str) -> Option<bool> {
        Some(is_url_host_and_has_path(url, "speedfiles.net", true, false))
    }

    async fn extract_video_url(from: ExtractFrom) -> Result<ExtractedVideo, anyhow::Error> {
        // Regex for base64 assignments
        static VIDEO_URL_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r#"(?:var|let|const) \w+ = ["']((?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{4}|[A-Za-z0-9+/]{3}=|[A-Za-z0-9+/]{2}={2}))["'];"#,
            )
            .unwrap()
        });

        fn decode_url(input: &str) -> Option<String> {
            // Decode base64
            let d = base64::prelude::BASE64_STANDARD.decode(input).ok()?;

            // Flip case of ascii letters and reverse vector
            let d = d
                .into_iter()
                .map(|x| {
                    if x.is_ascii_alphabetic() {
                        // https://stackoverflow.com/questions/42245397/c-most-efficient-way-to-change-uppercase-to-lowercase-and-vice-versa-without
                        x ^ 32
                    } else {
                        x
                    }
                })
                .rev()
                .collect::<Vec<_>>();

            // Decode base64 and reverse vector
            let mut d = base64::prelude::BASE64_STANDARD.decode(d).ok()?;
            d.reverse();

            // Parse hex to integer and subtract 3
            let d = d
                .chunks(2)
                .try_fold(Vec::with_capacity(d.len().div_ceil(2)), |mut acc, x| {
                    let hex = u8::from_str_radix(std::str::from_utf8(x).ok()?, 16).ok()?;
                    let sub = hex.checked_sub(3)?;
                    acc.push(sub);
                    Some(acc)
                })?;

            // Flip case of ascii letters and reverse vector
            let d = d
                .into_iter()
                .map(|x| {
                    if x.is_ascii_alphabetic() {
                        // https://stackoverflow.com/questions/42245397/c-most-efficient-way-to-change-uppercase-to-lowercase-and-vice-versa-without
                        x ^ 32
                    } else {
                        x
                    }
                })
                .rev()
                .collect::<Vec<_>>();

            // Decode base64
            let d = base64::prelude::BASE64_STANDARD.decode(d).ok()?;

            // As string
            let s = String::from_utf8(d).ok()?;

            if url::Url::parse(&s).is_err() {
                // If not a url, return None
                return None;
            }

            Some(s)
        }

        let source = from.get_source(None).await?;
        VIDEO_URL_REGEX
            .captures_iter(&source)
            .filter_map(|captures| captures.get(1))
            .filter_map(|capture| decode_url(capture.as_str()))
            .map(|video_url| ExtractedVideo {
                url: video_url,
                referer: None,
            })
            .next()
            .context("Speedfiles: failed to retrieve sources")
    }
}

#[cfg(test)]
mod tests {
    use super::Speedfiles;
    use crate::extractors::{ExtractFrom, Extractor};

    #[tokio::test]
    async fn test_speedfiles() {
        let url = "https://speedfiles.net/d2bb8bb75e7d";
        assert!(Speedfiles::supports_url(url).await.unwrap_or(false));

        let source = r#"<script>
var _0x5opu637 = "d6862e735a813f72a822d7bf4f06d95d6a562f72ca7dde7553d60da178ac5517";var _0x5opu126 = "b1e15d19a2f0ed8830b59629a68e4020e445ef893ac435a33da379db98bfa523e1786a0a729380f8f3fe0bb25696da021ad4baca42d88023fdf95d870601f612";var _0x5opu234 = "PT1hbldxZG0ycVpuV3V0eTNHWm4zQ2RtWm1NbjBtSm4weXRuV0Nkblp1Wm5YcUp6M0t0bktEdG0wbU1uS3p0eTNpZG5KemR6MWFabldxdHpacWRuS0RkejFhSm4weVpuM2FabTN5ZG4zQ1puV21aeTNHdG5XeWR6MXEybkpyZHozZVptMHF0eTFxMm5XdUp5M2F0bktEdG0xaTJtWnlabjBDdG5XcXR5MHFnbjNDWm4zcU1uNUN0eTJHWm5LRGRvM2F0blpxWm4weTJuNHlKejNLSm5IRHRuMUtabjVDWm4zS1ptWkNkbjJDWm5XQ1puM3FNbjNDdG5adXRuMnF0ejNHdG5LRGR6Wm1abjVtWm0zcU1uNUN0b1p1Sm40Q3R5M0NKbkhEdG4yS2RuMnFkelptMm1aQ2R6MUtabjVDWm4zbXRuNEN0b1p1ZG4wdXRvM3VabTJDWm0wdWduM0NabjNxMm0wcUpuMHEybkpyWnkwaWduS0RaeTBtTW5LelpuM3F3bjVDZG1aeWRuMnFKejNlWm0xQ1p5MGkybUpEdG8zcXduSm5aeTJlZ24zdWR6Wm1NbjJxWm4weTJtNHVkbTJ5d25LRHRvMmVnbjF1Wm0zcU1uSERkejB5Mm41eXR5MHl0bktuZG8zeVpuV210bjN1Sm4zQ0ptWktKbjV5SnkwQ3RuM0NkbzJlZ24wcWR6MktabTFDWnkxR1puSnpabjNhdG5IemRuMkNkbjN1ZG8zcUpuM0NkejFDWm1Kdlp5MnkybTVDZG0yaWduMm1abTN1Sm5JcmRu";var _0x5opu702 = "fc43c05509eb8def058bf87d2d13620cce496db91d000c2eba5afe67e4d009d0";var _0x5opu702 = "fc43c05509eb8def058bf87d2d13620cce496db91d000c2eba5afe67e4d009d0";var _0x2d04d9=_0x3d30;function _0x3d30(_0x6b524f,_0x1089b0){var _0x16881a=_0x281a();return _0x3d30=function(_0xf5f6c0,_0x20a792){_0xf5f6c0=_0xf5f6c0-0x171;var _0xcdd526=_0x16881a[_0xf5f6c0];return _0xcdd526;},_0x3d30(_0x6b524f,_0x1089b0);}(function(_0x3f2ad6,_0x3c2c61){var _0x2e810d=_0x3d30,_0x55e269=_0x3f2ad6();while(!![]){try{var _0x367ba6=-parseInt(_0x2e810d(0x1b1))/0x1*(parseInt(_0x2e810d(0x17a))/0x2)+parseInt(_0x2e810d(0x1b4))/0x3*(-parseInt(_0x2e810d(0x19b))/0x4)+-parseInt(_0x2e810d(0x178))/0x5+-parseInt(_0x2e810d(0x18a))/0x6+parseInt(_0x2e810d(0x1a3))/0x7+-parseInt(_0x2e810d(0x185))/0x8+parseInt(_0x2e810d(0x177))/0x9;if(_0x367ba6===_0x3c2c61)break;else _0x55e269['push'](_0x55e269['shift']());}catch(_0x439cd0){_0x55e269['push'](_0x55e269['shift']());}}}(_0x281a,0xda64b));function _0x281a(){var _0x300ca9=['replace','length','50000931KszqBp','7996525dGKBZv','wLuCW','4YgcZYi','[hQBquujEWhVQVvkcCLcuJDhPuaqSgSGjhRmP]','vjs-no-js','split','hQBsqpueedfuijEWhVlesQVvkcCL.cuneJDhPuaqtSgSGjhRmP','ITzCQ','innerHTML','rJAqr','ZSKnd','aRbouWtr:blankgRWgUBNUSAqxmOcXgcwXqAivYghWI','EcgZZ','6463936jiWtbV','preload','join','CLWJI','slice','4430118twybUN','RHnMN','HOJvQ','my-video','nIUpf','data-setup','auto','reverse','AcUFr','className','setAttribute','test','nVtVs','TGxCN','substr','QEwbI','apply','1764NdWNcb','poster','To\x20view\x20this\x20video\x20please\x20enable\x20JavaScript,\x20and\x20consider\x20upgrading\x20to\x20a\x20web\x20browser\x20that\x20<a\x20href=\x22https://videojs.com/html5-video-support/\x22\x20target=\x22_blank\x22>supports\x20HTML5\x20video</a>','type','toUpperCase','AepEB','fromCharCode','PzRhe','7665063ugzmne','addEventListener','szUVP','appendChild','ueIQQ','JdSEu','AKFIs','yPWfz','GOaWQ','createElement','src','controls','toLowerCase','return\x20(function()\x20','766558JOonFB','JhDXU','charCodeAt','7329HtSJzM','iRxGF','{}.constructor(\x22return\x20this\x22)(\x20)','kbsHg','WXboS','video','indexOf','wpEjp','QSZQp'];_0x281a=function(){return _0x300ca9;};return _0x281a();}var _0x20a792=(function(){var _0x1d1561=!![];return function(_0x1446b6,_0x2e9668){var _0x104ab1=_0x1d1561?function(){var _0x5e621a=_0x3d30;if(_0x5e621a(0x18b)===_0x5e621a(0x18b)){if(_0x2e9668){if('iQrst'!==_0x5e621a(0x174)){var _0x3c6f25=_0x2e9668[_0x5e621a(0x19a)](_0x1446b6,arguments);return _0x2e9668=null,_0x3c6f25;}else _0x34dbef=_0x923e0(_0x5e621a(0x1b0)+_0x5e621a(0x1b6)+');')();}}else _0x2074f7+=_0x28eeb1;}:function(){};return _0x1d1561=![],_0x104ab1;};}()),_0xf5f6c0=_0x20a792(this,function(){var _0xb7d7ff=_0x3d30,_0x3f20aa=function(){var _0x5b60c9=_0x3d30;if(_0x5b60c9(0x179)!==_0x5b60c9(0x18e)){var _0x22f14a;try{_0x5b60c9(0x1b2)===_0x5b60c9(0x199)?_0x4caca2+=_0x410df7[_0x5b60c9(0x1a1)](_0x2ead48(_0x3d9a87[_0x5b60c9(0x198)](_0x217602,0x2),0x10)):_0x22f14a=Function(_0x5b60c9(0x1b0)+_0x5b60c9(0x1b6)+');')();}catch(_0x531fde){_0x22f14a=window;}return _0x22f14a;}else _0x2be5f9=!![];},_0x380f2a=_0x3f20aa(),_0x48066a=new RegExp(_0xb7d7ff(0x17b),'g'),_0x4711c4=_0xb7d7ff(0x17e)['replace'](_0x48066a,'')[_0xb7d7ff(0x17d)](';'),_0x549060,_0x2d2eda,_0x206681,_0xfb8a1a,_0x58f1f3=function(_0x122f2f,_0x547066,_0x21ea2f){var _0x492b8c=_0xb7d7ff;if(_0x122f2f[_0x492b8c(0x176)]!=_0x547066)return![];for(var _0x1bfa97=0x0;_0x1bfa97<_0x547066;_0x1bfa97++){for(var _0x36e888=0x0;_0x36e888<_0x21ea2f[_0x492b8c(0x176)];_0x36e888+=0x2){if('uVXqX'==='wjONz'){if(_0xd1fdba){var _0x2ff09c=_0x39f879[_0x492b8c(0x19a)](_0x245dd0,arguments);return _0x1c2e9c=null,_0x2ff09c;}}else{if(_0x1bfa97==_0x21ea2f[_0x36e888]&&_0x122f2f['charCodeAt'](_0x1bfa97)!=_0x21ea2f[_0x36e888+0x1])return![];}}}return!![];},_0x34c7e7=function(_0xed6a1d,_0x39427c,_0x59764d){var _0x590dc7=_0xb7d7ff;if(_0x590dc7(0x1b8)!=='WXboS')return;else return _0x58f1f3(_0x39427c,_0x59764d,_0xed6a1d);},_0x939f90=function(_0x5e7566,_0x12fd8f,_0x458de6){var _0x8f671d=_0xb7d7ff;if('PGTOj'!==_0x8f671d(0x181))return _0x34c7e7(_0x12fd8f,_0x5e7566,_0x458de6);else{var _0x3a5a4f=_0x56373e[_0x2b3ac5],_0x3fddcf=_0x3a5a4f[0x0]===_0x5dbb86[_0x8f671d(0x1a1)](0x2e)?_0x3a5a4f[_0x8f671d(0x189)](0x1):_0x3a5a4f,_0x5a62a8=_0x2b40a8['length']-_0x3fddcf['length'],_0x45cbd9=_0x3d3975['indexOf'](_0x3fddcf,_0x5a62a8),_0x1a05af=_0x45cbd9!==-0x1&&_0x45cbd9===_0x5a62a8;_0x1a05af&&((_0x5ac13b[_0x8f671d(0x176)]==_0x3a5a4f[_0x8f671d(0x176)]||_0x3a5a4f['indexOf']('.')===0x0)&&(_0x2e7496=!![]));}},_0x54f795=function(_0x21f58c,_0x14f4a0,_0x55d2a2){var _0x10d86e=_0xb7d7ff;return _0x10d86e(0x1a9)!==_0x10d86e(0x1a9)?_0x333bca(_0xb655f5,_0x282b29,_0x387ba4):_0x939f90(_0x14f4a0,_0x55d2a2,_0x21f58c);};for(var _0x3ecaf2 in _0x380f2a){if(_0xb7d7ff(0x173)!==_0xb7d7ff(0x18c)){if(_0x58f1f3(_0x3ecaf2,0x8,[0x7,0x74,0x5,0x65,0x3,0x75,0x0,0x64])){if(_0xb7d7ff(0x184)!==_0xb7d7ff(0x196)){_0x549060=_0x3ecaf2;break;}else{var _0x4ca2d7=_0x44ffa3?function(){if(_0xd429d9){var _0x524cdf=_0x4bbe72['apply'](_0x12c08d,arguments);return _0x43c3fd=null,_0x524cdf;}}:function(){};return _0x401488=![],_0x4ca2d7;}}}else{if(_0x13b607==_0x115f74[_0x4249e2]&&_0x520dc7[_0xb7d7ff(0x1b3)](_0x4187a7)!=_0x84b65f[_0x57919e+0x1])return![];}}for(var _0xc0c865 in _0x380f2a[_0x549060]){if('EEMnP'!==_0xb7d7ff(0x1a0)){if(_0x54f795(0x6,_0xc0c865,[0x5,0x6e,0x0,0x64])){_0x2d2eda=_0xc0c865;break;}}else return _0x276428(_0x4e45a1,_0x55fb80,_0x1288c0);}for(var _0x4f189e in _0x380f2a[_0x549060]){if('NBTgN'===_0xb7d7ff(0x197)){var _0x125eed=_0xbb5c8a[_0x3bda6a];/[a-zA-Z]/['test'](_0x125eed)?_0x631c43+=_0x125eed===_0x125eed[_0xb7d7ff(0x1af)]()?_0x125eed['toUpperCase']():_0x125eed[_0xb7d7ff(0x1af)]():_0x246188+=_0x125eed;}else{if(_0x939f90(_0x4f189e,[0x7,0x6e,0x0,0x6c],0x8)){_0x206681=_0x4f189e;break;}}}if(!('~'>_0x2d2eda)){if(_0xb7d7ff(0x17f)==='ITzCQ')for(var _0x1c742a in _0x380f2a[_0x549060][_0x206681]){if(_0xb7d7ff(0x182)===_0xb7d7ff(0x182)){if(_0x34c7e7([0x7,0x65,0x0,0x68],_0x1c742a,0x8)){if(_0xb7d7ff(0x1a7)!==_0xb7d7ff(0x1b7)){_0xfb8a1a=_0x1c742a;break;}else return;}}else{var _0xb00cef;try{_0xb00cef=_0x3c1a53(_0xb7d7ff(0x1b0)+_0xb7d7ff(0x1b6)+');')();}catch(_0x1f1a0e){_0xb00cef=_0x675a94;}return _0xb00cef;}}else for(var _0x7d5f8b=0x0;_0x7d5f8b<_0x3b0ac8[_0xb7d7ff(0x176)];_0x7d5f8b+=0x2){if(_0x4e7ffc==_0x406934[_0x7d5f8b]&&_0x5450cb[_0xb7d7ff(0x1b3)](_0x72c70e)!=_0x36f7e7[_0x7d5f8b+0x1])return![];}}if(!_0x549060||!_0x380f2a[_0x549060])return;var _0x236477=_0x380f2a[_0x549060][_0x2d2eda],_0x53cc02=!!_0x380f2a[_0x549060][_0x206681]&&_0x380f2a[_0x549060][_0x206681][_0xfb8a1a],_0x3d833e=_0x236477||_0x53cc02;if(!_0x3d833e){if('cENzS'!=='uyMCO')return;else{var _0x59726f=_0x147b0e[_0x5b96d9];/[a-zA-Z]/[_0xb7d7ff(0x195)](_0x59726f)?_0x3ea9ee+=_0x59726f===_0x59726f[_0xb7d7ff(0x1af)]()?_0x59726f['toUpperCase']():_0x59726f[_0xb7d7ff(0x1af)]():_0xfd7706+=_0x59726f;}}var _0x4a1a64=![];for(var _0x47925f=0x0;_0x47925f<_0x4711c4[_0xb7d7ff(0x176)];_0x47925f++){if('KTyxA'===_0xb7d7ff(0x1a5))return _0x1cdde4(_0x40f6c8,_0x3ee66c,_0x4caa95);else{var _0x2d2eda=_0x4711c4[_0x47925f],_0x737ff8=_0x2d2eda[0x0]===String[_0xb7d7ff(0x1a1)](0x2e)?_0x2d2eda[_0xb7d7ff(0x189)](0x1):_0x2d2eda,_0x29932b=_0x3d833e[_0xb7d7ff(0x176)]-_0x737ff8['length'],_0x586ff7=_0x3d833e[_0xb7d7ff(0x172)](_0x737ff8,_0x29932b),_0x51a85c=_0x586ff7!==-0x1&&_0x586ff7===_0x29932b;if(_0x51a85c){if(_0x3d833e['length']==_0x2d2eda[_0xb7d7ff(0x176)]||_0x2d2eda[_0xb7d7ff(0x172)]('.')===0x0){if(_0xb7d7ff(0x1a8)===_0xb7d7ff(0x1a8))_0x4a1a64=!![];else{var _0xc784=new _0x38d923('[RWrgRWgUBNUSAqxmOcXgcwXqAivYghWI]','g'),_0x2cbdc1=_0xb7d7ff(0x183)[_0xb7d7ff(0x175)](_0xc784,'');_0x5695bb[_0xb0a580][_0xa467e3]=_0x2cbdc1;}}}}}if(!_0x4a1a64){var _0x3f3f9b=new RegExp('[RWrgRWgUBNUSAqxmOcXgcwXqAivYghWI]','g'),_0x26c5ab=_0xb7d7ff(0x183)['replace'](_0x3f3f9b,'');_0x380f2a[_0x549060][_0x206681]=_0x26c5ab;}});_0xf5f6c0(),window[_0x2d04d9(0x1a4)]('load',function(){var _0x146fff=_0x2d04d9,_0x21e993=atob(_0x5opu234),_0x680b15='';for(var _0x5346d9=0x0;_0x5346d9<_0x21e993['length'];_0x5346d9++){if(_0x146fff(0x1a2)===_0x146fff(0x1a2)){var _0x8aa331=_0x21e993[_0x5346d9];/[a-zA-Z]/[_0x146fff(0x195)](_0x8aa331)?_0x680b15+=_0x8aa331===_0x8aa331[_0x146fff(0x1af)]()?_0x8aa331[_0x146fff(0x19f)]():_0x8aa331['toLowerCase']():_0x146fff(0x1ab)!=='GOaWQ'?_0xf02262+=_0x82afe7:_0x680b15+=_0x8aa331;}else _0x1b88f8+=_0xce1f0d===_0x31acbe['toLowerCase']()?_0x2a1461['toUpperCase']():_0x288056[_0x146fff(0x1af)]();}var _0x51130b=_0x680b15['split']('')[_0x146fff(0x191)]()[_0x146fff(0x187)](''),_0x2e498d=atob(_0x51130b),_0x10c983=_0x2e498d[_0x146fff(0x17d)]('')[_0x146fff(0x191)]()['join'](''),_0x36c833='';for(var _0x5346d9=0x0;_0x5346d9<_0x10c983['length'];_0x5346d9+=0x2){'qmMvS'===_0x146fff(0x188)?_0x311a72=_0xc05f89:_0x36c833+=String['fromCharCode'](parseInt(_0x10c983['substr'](_0x5346d9,0x2),0x10));}var _0xa7a6bb='';for(var _0x5346d9=0x0;_0x5346d9<_0x36c833['length'];_0x5346d9++){if('sJxtU'!==_0x146fff(0x1b5))_0xa7a6bb+=String[_0x146fff(0x1a1)](_0x36c833[_0x146fff(0x1b3)](_0x5346d9)-0x3);else return![];}var _0x543e4f='';for(var _0x5346d9=0x0;_0x5346d9<_0xa7a6bb[_0x146fff(0x176)];_0x5346d9++){if('kimQO'==='kimQO'){var _0x8aa331=_0xa7a6bb[_0x5346d9];if(/[a-zA-Z]/[_0x146fff(0x195)](_0x8aa331))_0x543e4f+=_0x8aa331===_0x8aa331[_0x146fff(0x1af)]()?_0x8aa331[_0x146fff(0x19f)]():_0x8aa331['toLowerCase']();else{if(_0x146fff(0x192)!==_0x146fff(0x1aa))_0x543e4f+=_0x8aa331;else return![];}}else _0x83053e+=_0x14f280[_0x146fff(0x1a1)](_0x5f19f7[_0x146fff(0x1b3)](_0x7f77c0)-0x3);}var _0x455c6d=_0x543e4f[_0x146fff(0x17d)]('')[_0x146fff(0x191)]()[_0x146fff(0x187)](''),_0x15be99=atob(_0x455c6d),_0x5e8295=document[_0x146fff(0x1ac)](_0x146fff(0x171));_0x5e8295['id']=_0x146fff(0x18d),_0x5e8295[_0x146fff(0x193)]='video-js\x20vjs-default-skin',_0x5e8295[_0x146fff(0x194)](_0x146fff(0x1ae),''),_0x5e8295[_0x146fff(0x194)](_0x146fff(0x186),_0x146fff(0x190)),_0x5e8295['setAttribute'](_0x146fff(0x19c),'/assets/poster.jpg'),_0x5e8295[_0x146fff(0x194)](_0x146fff(0x18f),'{}');var _0x56c226=document[_0x146fff(0x1ac)]('source');_0x56c226[_0x146fff(0x1ad)]=_0x15be99,_0x56c226[_0x146fff(0x19e)]='video/mp4',_0x5e8295[_0x146fff(0x1a6)](_0x56c226);var _0x4a3a8f=document[_0x146fff(0x1ac)]('p');_0x4a3a8f[_0x146fff(0x193)]=_0x146fff(0x17c),_0x4a3a8f[_0x146fff(0x180)]=_0x146fff(0x19d),_0x5e8295[_0x146fff(0x1a6)](_0x4a3a8f),document['body'][_0x146fff(0x1a6)](_0x5e8295),videojs(_0x146fff(0x18d));});
    document.addEventListener('contextmenu', (event) => event.preventDefault()); document.addEventListener('keydown', (event) => { if (event.key === "F12" || (event.ctrlKey && event.shiftKey && event.key === "I") || (event.ctrlKey && event.shiftKey && event.key === "J") || (event.ctrlKey && event.shiftKey && event.key === "C") || (event.ctrlKey && event.key === "U") || (event.ctrlKey && event.key === "S")) { event.preventDefault(); } });
</script>"#;
        let expected = "https://md4.t0006.cache-tqz84v1.speedfiles.net/store_access/d2bb8bb75e7d?token=5ogOcpTFMa6TVtWFsRK6D6S044U5oDeKRWi1FDXVSv8&t=1731385663&e=10800&f=d2bb8bb75e7d&sp=1500";

        let extracted = Speedfiles::extract_video_url(ExtractFrom::Source(source.to_string())).await;
        assert_eq!(extracted.unwrap().url, expected.to_string());
    }
}
