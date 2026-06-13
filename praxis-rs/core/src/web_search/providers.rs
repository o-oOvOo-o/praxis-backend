use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) enum SearchProviderKind {
    BingCn,
    BingUs,
    BingUk,
    BingJp,
    BingRu,
    BingEg,
    BingEgEn,
    BingZa,
    BingNg,
    BingKe,
    BingGh,
    BingMa,
    BingTz,
    BingUg,
    BingMobileCn,
    BingMobileUs,
    BingMobileJp,
    BingMobileRu,
    BingMobileEg,
    BingMobileEgEn,
    BingMobileZa,
    BingMobileNg,
    BingMobileKe,
    BingMobileGh,
    BingMobileMa,
    BingMobileTz,
    BingMobileUg,
    DuckDuckGoHtml,
    GoogleUs,
    GoogleZhCn,
    GoogleJa,
    GoogleRu,
    GoogleEg,
    GoogleEgEn,
    GoogleZa,
    GoogleNg,
    GoogleKe,
    GoogleGh,
    GoogleMa,
    GoogleTz,
    GoogleUg,
    Baidu,
    BaiduMobile,
    Sogou,
    SogouMobile,
    SogouWeixin,
    So360,
    ShenmaMobile,
    Toutiao,
    BaiduNews,
    Yandex,
    Yahoo,
    YahooJapan,
    Naver,
    Qwant,
    Mojeek,
    Startpage,
}

impl SearchProviderKind {
    pub(super) fn all() -> &'static [Self] {
        &[
            Self::BingCn,
            Self::BingUs,
            Self::BingUk,
            Self::BingJp,
            Self::BingRu,
            Self::BingEg,
            Self::BingEgEn,
            Self::BingZa,
            Self::BingNg,
            Self::BingKe,
            Self::BingGh,
            Self::BingMa,
            Self::BingTz,
            Self::BingUg,
            Self::BingMobileCn,
            Self::BingMobileUs,
            Self::BingMobileJp,
            Self::BingMobileRu,
            Self::BingMobileEg,
            Self::BingMobileEgEn,
            Self::BingMobileZa,
            Self::BingMobileNg,
            Self::BingMobileKe,
            Self::BingMobileGh,
            Self::BingMobileMa,
            Self::BingMobileTz,
            Self::BingMobileUg,
            Self::DuckDuckGoHtml,
            Self::GoogleUs,
            Self::GoogleZhCn,
            Self::GoogleJa,
            Self::GoogleRu,
            Self::GoogleEg,
            Self::GoogleEgEn,
            Self::GoogleZa,
            Self::GoogleNg,
            Self::GoogleKe,
            Self::GoogleGh,
            Self::GoogleMa,
            Self::GoogleTz,
            Self::GoogleUg,
            Self::Baidu,
            Self::BaiduMobile,
            Self::Sogou,
            Self::SogouMobile,
            Self::SogouWeixin,
            Self::So360,
            Self::ShenmaMobile,
            Self::Toutiao,
            Self::BaiduNews,
            Self::Yandex,
            Self::Yahoo,
            Self::YahooJapan,
            Self::Naver,
            Self::Qwant,
            Self::Mojeek,
            Self::Startpage,
        ]
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::BingCn => "obscura_bing_cn",
            Self::BingUs => "obscura_bing_us",
            Self::BingUk => "obscura_bing_uk",
            Self::BingJp => "obscura_bing_ja_jp",
            Self::BingRu => "obscura_bing_ru_ru",
            Self::BingEg => "obscura_bing_eg",
            Self::BingEgEn => "obscura_bing_eg_en",
            Self::BingZa => "obscura_bing_za",
            Self::BingNg => "obscura_bing_ng",
            Self::BingKe => "obscura_bing_ke",
            Self::BingGh => "obscura_bing_gh",
            Self::BingMa => "obscura_bing_ma",
            Self::BingTz => "obscura_bing_tz",
            Self::BingUg => "obscura_bing_ug",
            Self::BingMobileCn => "obscura_bing_mobile_cn",
            Self::BingMobileUs => "obscura_bing_mobile_us",
            Self::BingMobileJp => "obscura_bing_mobile_ja_jp",
            Self::BingMobileRu => "obscura_bing_mobile_ru_ru",
            Self::BingMobileEg => "obscura_bing_mobile_eg",
            Self::BingMobileEgEn => "obscura_bing_mobile_eg_en",
            Self::BingMobileZa => "obscura_bing_mobile_za",
            Self::BingMobileNg => "obscura_bing_mobile_ng",
            Self::BingMobileKe => "obscura_bing_mobile_ke",
            Self::BingMobileGh => "obscura_bing_mobile_gh",
            Self::BingMobileMa => "obscura_bing_mobile_ma",
            Self::BingMobileTz => "obscura_bing_mobile_tz",
            Self::BingMobileUg => "obscura_bing_mobile_ug",
            Self::DuckDuckGoHtml => "obscura_duckduckgo_html",
            Self::GoogleUs => "obscura_google_us",
            Self::GoogleZhCn => "obscura_google_zh_cn",
            Self::GoogleJa => "obscura_google_ja_jp",
            Self::GoogleRu => "obscura_google_ru_ru",
            Self::GoogleEg => "obscura_google_eg",
            Self::GoogleEgEn => "obscura_google_eg_en",
            Self::GoogleZa => "obscura_google_za",
            Self::GoogleNg => "obscura_google_ng",
            Self::GoogleKe => "obscura_google_ke",
            Self::GoogleGh => "obscura_google_gh",
            Self::GoogleMa => "obscura_google_ma",
            Self::GoogleTz => "obscura_google_tz",
            Self::GoogleUg => "obscura_google_ug",
            Self::Baidu => "obscura_baidu",
            Self::BaiduMobile => "obscura_baidu_mobile",
            Self::Sogou => "obscura_sogou",
            Self::SogouMobile => "obscura_sogou_mobile",
            Self::SogouWeixin => "obscura_sogou_weixin",
            Self::So360 => "obscura_360",
            Self::ShenmaMobile => "obscura_shenma_mobile",
            Self::Toutiao => "obscura_toutiao",
            Self::BaiduNews => "obscura_baidu_news",
            Self::Yandex => "obscura_yandex",
            Self::Yahoo => "obscura_yahoo",
            Self::YahooJapan => "obscura_yahoo_japan",
            Self::Naver => "obscura_naver",
            Self::Qwant => "obscura_qwant",
            Self::Mojeek => "obscura_mojeek",
            Self::Startpage => "obscura_startpage",
        }
    }

    pub(super) fn search_url(self, query: &str) -> String {
        let encoded = encode_query(query);
        match self {
            Self::BingCn => Self::bing_url(
                "https://cn.bing.com/search",
                &encoded,
                "zh-CN",
                "zh-CN",
                "CN",
            ),
            Self::BingUs => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-US", "en", "US")
            }
            Self::BingUk => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GB", "en", "GB")
            }
            Self::BingJp => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ja-JP", "ja", "JP")
            }
            Self::BingRu => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ru-RU", "ru", "RU")
            }
            Self::BingEg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ar-EG", "ar", "EG")
            }
            Self::BingEgEn => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-EG", "en", "EG")
            }
            Self::BingZa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-ZA", "en", "ZA")
            }
            Self::BingNg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-NG", "en", "NG")
            }
            Self::BingKe => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-KE", "en", "KE")
            }
            Self::BingGh => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GH", "en", "GH")
            }
            Self::BingMa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-MA", "en", "MA")
            }
            Self::BingTz => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-TZ", "en", "TZ")
            }
            Self::BingUg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-UG", "en", "UG")
            }
            Self::BingMobileCn => Self::bing_url(
                "https://cn.bing.com/search",
                &encoded,
                "zh-CN",
                "zh-CN",
                "CN",
            ),
            Self::BingMobileUs => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-US", "en", "US")
            }
            Self::BingMobileJp => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ja-JP", "ja", "JP")
            }
            Self::BingMobileRu => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ru-RU", "ru", "RU")
            }
            Self::BingMobileEg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ar-EG", "ar", "EG")
            }
            Self::BingMobileEgEn => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-EG", "en", "EG")
            }
            Self::BingMobileZa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-ZA", "en", "ZA")
            }
            Self::BingMobileNg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-NG", "en", "NG")
            }
            Self::BingMobileKe => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-KE", "en", "KE")
            }
            Self::BingMobileGh => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GH", "en", "GH")
            }
            Self::BingMobileMa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-MA", "en", "MA")
            }
            Self::BingMobileTz => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-TZ", "en", "TZ")
            }
            Self::BingMobileUg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-UG", "en", "UG")
            }
            Self::DuckDuckGoHtml => format!("https://html.duckduckgo.com/html/?q={encoded}"),
            Self::GoogleUs => Self::google_url(&encoded, "en", "us"),
            Self::GoogleZhCn => Self::google_url(&encoded, "zh-CN", "cn"),
            Self::GoogleJa => Self::google_url(&encoded, "ja", "jp"),
            Self::GoogleRu => Self::google_url(&encoded, "ru", "ru"),
            Self::GoogleEg => {
                Self::google_country_url("https://www.google.com.eg/search", &encoded, "ar", "eg")
            }
            Self::GoogleEgEn => {
                Self::google_country_url("https://www.google.com.eg/search", &encoded, "en", "eg")
            }
            Self::GoogleZa => {
                Self::google_country_url("https://www.google.co.za/search", &encoded, "en", "za")
            }
            Self::GoogleNg => {
                Self::google_country_url("https://www.google.com.ng/search", &encoded, "en", "ng")
            }
            Self::GoogleKe => {
                Self::google_country_url("https://www.google.co.ke/search", &encoded, "en", "ke")
            }
            Self::GoogleGh => {
                Self::google_country_url("https://www.google.com.gh/search", &encoded, "en", "gh")
            }
            Self::GoogleMa => {
                Self::google_country_url("https://www.google.co.ma/search", &encoded, "en", "ma")
            }
            Self::GoogleTz => {
                Self::google_country_url("https://www.google.co.tz/search", &encoded, "en", "tz")
            }
            Self::GoogleUg => {
                Self::google_country_url("https://www.google.co.ug/search", &encoded, "en", "ug")
            }
            Self::Baidu => {
                format!("https://www.baidu.com/s?wd={encoded}&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::BaiduMobile => {
                format!("https://m.baidu.com/s?word={encoded}&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::Sogou => {
                format!("https://www.sogou.com/web?query={encoded}&num={SEARCH_RESULT_LIMIT}")
            }
            Self::SogouMobile => {
                format!("https://m.sogou.com/web/searchList.jsp?keyword={encoded}")
            }
            Self::SogouWeixin => format!("https://weixin.sogou.com/weixin?type=2&query={encoded}"),
            Self::So360 => format!("https://www.so.com/s?q={encoded}"),
            Self::ShenmaMobile => format!("https://m.sm.cn/s?q={encoded}"),
            Self::Toutiao => format!("https://so.toutiao.com/search?keyword={encoded}"),
            Self::BaiduNews => {
                format!("https://news.baidu.com/ns?word={encoded}&tn=news&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::Yandex => format!("https://yandex.com/search/?text={encoded}"),
            Self::Yahoo => format!("https://search.yahoo.com/search?p={encoded}"),
            Self::YahooJapan => format!("https://search.yahoo.co.jp/search?p={encoded}"),
            Self::Naver => format!("https://search.naver.com/search.naver?query={encoded}"),
            Self::Qwant => format!("https://www.qwant.com/?q={encoded}&t=web"),
            Self::Mojeek => format!("https://www.mojeek.com/search?q={encoded}"),
            Self::Startpage => format!("https://www.startpage.com/sp/search?query={encoded}"),
        }
    }

    pub(super) fn bing_url(
        base: &str,
        encoded: &str,
        market: &str,
        language: &str,
        country: &str,
    ) -> String {
        format!(
            "{base}?q={encoded}&count={SEARCH_RESULT_LIMIT}&mkt={market}&setlang={language}&cc={country}"
        )
    }

    pub(super) fn google_url(encoded: &str, language: &str, country: &str) -> String {
        Self::google_country_url("https://www.google.com/search", encoded, language, country)
    }

    pub(super) fn google_country_url(
        base: &str,
        encoded: &str,
        language: &str,
        country: &str,
    ) -> String {
        format!("{base}?q={encoded}&num={SEARCH_RESULT_LIMIT}&hl={language}&gl={country}&pws=0")
    }

    pub(super) fn user_agent(self) -> &'static str {
        match self {
            Self::BingMobileCn
            | Self::BingMobileUs
            | Self::BingMobileJp
            | Self::BingMobileRu
            | Self::BingMobileEg
            | Self::BingMobileEgEn
            | Self::BingMobileZa
            | Self::BingMobileNg
            | Self::BingMobileKe
            | Self::BingMobileGh
            | Self::BingMobileMa
            | Self::BingMobileTz
            | Self::BingMobileUg
            | Self::BaiduMobile
            | Self::SogouMobile
            | Self::ShenmaMobile => SEARCH_MOBILE_USER_AGENT,
            _ => SEARCH_USER_AGENT,
        }
    }
}
