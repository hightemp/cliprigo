//! YouTube search filters. yt-dlp's `ytsearch` takes none, so searches go through
//! the YouTube results page, whose `sp` parameter carries the filters as a base64
//! protobuf message: field 1 is the order, field 2 the filters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    #[serde(default)]
    pub upload_date: Option<UploadDate>,
    #[serde(default)]
    pub length: Option<Length>,
    #[serde(default)]
    pub features: Vec<Feature>,
    #[serde(default)]
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadDate {
    Hour,
    Today,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    /// Under 4 minutes.
    Short,
    /// 4 to 20 minutes.
    Medium,
    /// Over 20 minutes.
    Long,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Hd,
    Subtitles,
    #[serde(rename = "4k")]
    Uhd,
    Hdr,
}

/// YouTube no longer honors "by upload date" or "by rating" here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Relevance,
    Views,
}

/// The `sp` parameter for `filters`.
pub fn params(filters: &SearchFilters) -> String {
    use base64::Engine;

    let mut inner = Vec::new();
    if let Some(date) = filters.upload_date {
        let value = match date {
            UploadDate::Hour => 1,
            UploadDate::Today => 2,
            UploadDate::Week => 3,
            UploadDate::Month => 4,
            UploadDate::Year => 5,
        };
        field(&mut inner, 1, value);
    }
    // Type "video": channels and playlists can't be downloaded as one file.
    field(&mut inner, 2, 1);
    if let Some(length) = filters.length {
        let value = match length {
            Length::Short => 1,
            Length::Long => 2,
            Length::Medium => 3,
        };
        field(&mut inner, 3, value);
    }
    let mut features: Vec<u64> = filters
        .features
        .iter()
        .map(|f| match f {
            Feature::Hd => 4,
            Feature::Subtitles => 5,
            Feature::Uhd => 14,
            Feature::Hdr => 25,
        })
        .collect();
    // Same filters, same request: fields in order, each once.
    features.sort_unstable();
    features.dedup();
    for number in features {
        field(&mut inner, number, 1);
    }

    let mut message = Vec::new();
    if filters.sort == Sort::Views {
        field(&mut message, 1, 3);
    }
    varint(&mut message, 2 << 3 | 2);
    varint(&mut message, inner.len() as u64);
    message.extend(inner);
    base64::engine::general_purpose::STANDARD.encode(message)
}

/// Results page for `query` with `filters`, for yt-dlp to list.
pub fn results_url(query: &str, filters: &SearchFilters) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("search_query", query)
        .append_pair("sp", &params(filters))
        .finish();
    format!("https://www.youtube.com/results?{query}")
}

/// A protobuf varint field.
fn field(out: &mut Vec<u8>, number: u64, value: u64) {
    varint(out, number << 3);
    varint(out, value);
}

fn varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values checked against YouTube with yt-dlp.
    #[test]
    fn plain_search_asks_for_videos_only() {
        assert_eq!(params(&SearchFilters::default()), "EgIQAQ==");
    }

    #[test]
    fn filters_combine() {
        let views = SearchFilters {
            sort: Sort::Views,
            ..Default::default()
        };
        assert_eq!(params(&views), "CAMSAhAB");

        let recent_short = SearchFilters {
            upload_date: Some(UploadDate::Week),
            length: Some(Length::Short),
            ..Default::default()
        };
        assert_eq!(params(&recent_short), "EgYIAxABGAE=");

        let long_4k = SearchFilters {
            length: Some(Length::Long),
            features: vec![Feature::Uhd],
            ..Default::default()
        };
        assert_eq!(params(&long_4k), "EgYQARgCcAE=");

        let subtitles = SearchFilters {
            features: vec![Feature::Subtitles],
            ..Default::default()
        };
        assert_eq!(params(&subtitles), "EgQQASgB");

        let hdr = SearchFilters {
            features: vec![Feature::Hdr],
            ..Default::default()
        };
        assert_eq!(params(&hdr), "EgUQAcgBAQ==");
    }

    #[test]
    fn features_keep_one_order() {
        let a = SearchFilters {
            features: vec![Feature::Hdr, Feature::Hd, Feature::Hdr],
            ..Default::default()
        };
        let b = SearchFilters {
            features: vec![Feature::Hd, Feature::Hdr],
            ..Default::default()
        };
        assert_eq!(params(&a), params(&b));
    }

    #[test]
    fn url_escapes_the_query() {
        assert_eq!(
            results_url("rick & morty?", &SearchFilters::default()),
            "https://www.youtube.com/results?search_query=rick+%26+morty%3F&sp=EgIQAQ%3D%3D"
        );
    }

    #[test]
    fn filters_come_from_the_ui() {
        let filters: SearchFilters = serde_json::from_str(
            r#"{"uploadDate":"month","length":"medium","features":["4k","hdr"],"sort":"views"}"#,
        )
        .unwrap();
        assert_eq!(filters.upload_date, Some(UploadDate::Month));
        assert_eq!(filters.length, Some(Length::Medium));
        assert_eq!(filters.features, [Feature::Uhd, Feature::Hdr]);
        assert_eq!(filters.sort, Sort::Views);
        let none: SearchFilters = serde_json::from_str("{}").unwrap();
        assert_eq!(none, SearchFilters::default());
    }
}
