use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use dom_query::Document;
use encoding_rs::Encoding;
use image::{codecs::jpeg::JpegEncoder, ImageFormat, ImageReader, Limits};
use reqwest::{header, redirect::Policy, Client, Response};
use serde::Deserialize;
use url::{Host, Url};

use crate::{error::AppError, models::WebLinkMetadata, normalize::validate_and_normalize_url};

const HTML_BYTES: usize = 512 * 1024;
const IMAGE_BYTES: usize = 6 * 1024 * 1024;
const THUMBNAIL_BYTES: usize = 256 * 1024;
const JPEG_DATA_URL: &str = "data:image/jpeg;base64,";

pub async fn fetch(input: &str) -> Result<WebLinkMetadata, AppError> {
    let normalized = validate_and_normalize_url(input)?;
    let url = Url::parse(&normalized.normalized_url).expect("validated WebLink URL");
    let page = tokio::time::timeout(Duration::from_secs(4), fetch_page(url)).await;
    let Ok(Some(page)) = page else {
        return Ok(WebLinkMetadata::default());
    };

    let thumbnail = match page.image_url {
        Some(url) => tokio::time::timeout(Duration::from_secs(3), fetch_thumbnail(url))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    Ok(WebLinkMetadata {
        title: page.title,
        description: page.description,
        thumbnail,
    })
}

struct PageMetadata {
    title: Option<String>,
    description: Option<String>,
    image_url: Option<Url>,
}

async fn fetch_page(url: Url) -> Option<PageMetadata> {
    if let Some(endpoint) = youtube_oembed_url(&url) {
        if let Ok(Some(video)) =
            tokio::time::timeout(Duration::from_secs(2), fetch_youtube_oembed(endpoint)).await
        {
            return Some(video);
        }
    }
    let (resolved_url, response) = request(url, "text/html,application/xhtml+xml").await?;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if let Some(media_type) = content_type
        .as_deref()
        .and_then(|value| value.split(';').next())
    {
        if !["text/html", "application/xhtml+xml"]
            .iter()
            .any(|kind| media_type.trim().eq_ignore_ascii_case(kind))
        {
            return None;
        }
    }
    let html = read_body(response, HTML_BYTES, true).await?;
    tauri::async_runtime::spawn_blocking(move || {
        parse_page(&resolved_url, &html, content_type.as_deref())
    })
    .await
    .ok()
}

fn youtube_oembed_url(url: &Url) -> Option<Url> {
    if !is_web_url(url) {
        return None;
    }
    let mut segments = url.path_segments()?;
    let video = match url.host_str()?.trim_end_matches('.') {
        "youtu.be" => segments.next()?.to_owned(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            match segments.next()? {
                "watch" => url
                    .query_pairs()
                    .find(|(name, _)| name == "v")?
                    .1
                    .into_owned(),
                "shorts" | "live" | "embed" => segments.next()?.to_owned(),
                _ => return None,
            }
        }
        _ => return None,
    };
    if video.len() != 11
        || !video
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    let mut endpoint = Url::parse("https://www.youtube.com/oembed").ok()?;
    let watch_url = format!("https://www.youtube.com/watch?v={video}");
    endpoint
        .query_pairs_mut()
        .append_pair("url", &watch_url)
        .append_pair("format", "json");
    Some(endpoint)
}

async fn fetch_youtube_oembed(endpoint: Url) -> Option<PageMetadata> {
    #[derive(Deserialize)]
    struct Video {
        title: String,
        thumbnail_url: Option<String>,
    }

    let (_, response) = request(endpoint, "application/json").await?;
    let data = read_body(response, 32 * 1024, false).await?;
    let video: Video = serde_json::from_slice(&data).ok()?;
    Some(PageMetadata {
        title: clean_text(&video.title, 500),
        description: None,
        image_url: video
            .thumbnail_url
            .and_then(|raw| Url::parse(&raw).ok())
            .filter(is_web_url),
    })
}

async fn fetch_thumbnail(url: Url) -> Option<String> {
    let (_, response) = request(url, "image/jpeg,image/png,image/webp").await?;
    let image = read_body(response, IMAGE_BYTES, false).await?;
    tauri::async_runtime::spawn_blocking(move || make_thumbnail(&image))
        .await
        .ok()
        .flatten()
}

async fn request(mut url: Url, accept: &'static str) -> Option<(Url, Response)> {
    for _ in 0..=4 {
        if !is_web_url(&url) {
            return None;
        }
        let port = url.port_or_known_default()?;
        let mut builder = Client::builder()
            .tls_backend_native()
            .no_proxy()
            .redirect(Policy::none())
            .referer(false)
            .connect_timeout(Duration::from_secs(2))
            .user_agent("Oboromi/0.1");
        match url.host()? {
            Host::Domain(host) => {
                if !host.trim_end_matches('.').contains('.') {
                    return None;
                }
                let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
                    .await
                    .ok()?
                    .filter(|address| is_public_ip(address.ip()))
                    .collect();
                if addresses.is_empty() {
                    return None;
                }
                // Use only the addresses checked above, including when a redirect changes hosts.
                builder = builder.resolve_to_addrs(host, &addresses);
            }
            Host::Ipv4(ip) if !is_public_ip(ip.into()) => return None,
            Host::Ipv6(ip) if !is_public_ip(ip.into()) => return None,
            _ => {}
        }

        let response = builder
            .build()
            .ok()?
            .get(url.clone())
            .header(header::ACCEPT, accept)
            .send()
            .await
            .ok()?;
        if response.status().is_redirection() {
            let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
            url = url.join(location).ok()?;
        } else {
            return response.status().is_success().then_some((url, response));
        }
    }
    None
}

fn is_web_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            a != 0
                && a < 224
                && !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_documentation()
                && !matches!(
                    (a, b, c),
                    (100, 64..=127, _) | (192, 0, 0) | (192, 88, 99) | (198, 18..=19, _)
                )
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_ip(mapped.into());
            }
            let [a, b, ..] = ip.segments();
            // Global unicast, excluding documentation, legacy transition and protocol ranges.
            (0x2000..=0x3fff).contains(&a)
                && a != 0x2002
                && a != 0x3fff
                && !(a == 0x2001 && (b <= 0x01ff || b == 0x0db8))
        }
    }
}

async fn read_body(mut response: Response, limit: usize, allow_prefix: bool) -> Option<Vec<u8>> {
    if !allow_prefix
        && response
            .content_length()
            .is_some_and(|length| length > limit as u64)
    {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        let remaining = limit - body.len();
        if !allow_prefix && chunk.len() > remaining {
            return None;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if allow_prefix && body.len() == limit {
            break;
        }
    }
    Some(body)
}

fn parse_page(url: &Url, bytes: &[u8], content_type: Option<&str>) -> PageMetadata {
    let encoding = Encoding::for_bom(bytes)
        .map(|(encoding, _)| encoding)
        .or_else(|| content_type.and_then(charset));
    let document = match encoding {
        Some(encoding) => Document::from(encoding.decode(bytes).0.as_ref()),
        None => {
            let document = Document::from(String::from_utf8_lossy(bytes).as_ref());
            let declared = document
                .select_single("meta[charset]")
                .attr("charset")
                .and_then(|label| Encoding::for_label(label.as_bytes()))
                .or_else(|| {
                    document
                        .select_single("meta[http-equiv='content-type' i]")
                        .attr("content")
                        .and_then(|value| charset(&value))
                });
            match declared {
                Some(encoding) if encoding != encoding_rs::UTF_8 => {
                    Document::from(encoding.decode(bytes).0.as_ref())
                }
                _ => document,
            }
        }
    };
    let title = meta(
        &document,
        &[
            "meta[property='og:title' i]",
            "meta[name='twitter:title' i]",
        ],
        500,
    )
    .or_else(|| clean_text(&document.select_single("title").text(), 500));
    let description = meta(
        &document,
        &[
            "meta[property='og:description' i]",
            "meta[name='description' i]",
            "meta[name='twitter:description' i]",
        ],
        1500,
    );
    let image_url = [
        "meta[property='og:image:secure_url' i]",
        "meta[property='og:image' i]",
        "meta[property='og:image:url' i]",
        "meta[name='twitter:image' i]",
        "meta[name='twitter:image:src' i]",
        "link[rel~='image_src' i]",
    ]
    .iter()
    .find_map(|selector| {
        let field = if selector.starts_with("link") {
            "href"
        } else {
            "content"
        };
        document
            .select(selector)
            .nodes()
            .iter()
            .filter_map(|node| node.attr(field))
            .filter(|value| !value.trim().is_empty())
            .filter_map(|value| url.join(value.trim()).ok())
            .find(is_web_url)
    });
    PageMetadata {
        title,
        description,
        image_url,
    }
}

fn charset(content_type: &str) -> Option<&'static Encoding> {
    content_type
        .split(';')
        .skip(1)
        .filter_map(|part| part.split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("charset"))
        .and_then(|(_, value)| {
            Encoding::for_label(value.trim().trim_matches(['"', '\'']).as_bytes())
        })
}

fn meta(document: &Document, selectors: &[&str], limit: usize) -> Option<String> {
    selectors.iter().find_map(|selector| {
        document
            .select(selector)
            .nodes()
            .iter()
            .filter_map(|node| node.attr("content"))
            .find_map(|value| clean_text(&value, limit))
    })
}

fn clean_text(value: &str, limit: usize) -> Option<String> {
    let text: String = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect();
    (!text.is_empty()).then_some(text)
}

fn make_thumbnail(bytes: &[u8]) -> Option<String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format()?,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let preview = image.thumbnail(640, 480).to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 82)
        .encode_image(&preview)
        .ok()?;
    (jpeg.len() <= THUMBNAIL_BYTES).then(|| thumbnail_data_url(&jpeg))
}

pub(crate) fn thumbnail_data_url(jpeg: &[u8]) -> String {
    format!("{JPEG_DATA_URL}{}", STANDARD.encode(jpeg))
}

pub(crate) fn decode_thumbnail(value: &str) -> Result<Vec<u8>, AppError> {
    let bytes = value
        .strip_prefix(JPEG_DATA_URL)
        .filter(|raw| raw.len() <= THUMBNAIL_BYTES.div_ceil(3) * 4)
        .and_then(|raw| STANDARD.decode(raw).ok())
        .filter(|jpeg| {
            jpeg.len() <= THUMBNAIL_BYTES
                && jpeg.starts_with(&[0xff, 0xd8, 0xff])
                && jpeg.ends_with(&[0xff, 0xd9])
        })
        .filter(|jpeg| {
            ImageReader::with_format(Cursor::new(jpeg), ImageFormat::Jpeg)
                .into_dimensions()
                .is_ok_and(|(width, height)| {
                    width > 0 && width <= 640 && height > 0 && height <= 480
                })
        });
    bytes.ok_or_else(|| AppError::Validation("サムネイルの形式が正しくありません".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_preview_uses_the_public_video_id() {
        for url in [
            "https://www.youtube.com/watch?v=abcDEF123_-&list=sample",
            "https://youtu.be/abcDEF123_-?t=10",
            "https://www.youtube.com/shorts/abcDEF123_-",
        ] {
            let endpoint = youtube_oembed_url(&Url::parse(url).unwrap()).unwrap();
            assert_eq!(endpoint.path(), "/oembed");
            assert_eq!(
                endpoint.query_pairs().next().unwrap().1,
                "https://www.youtube.com/watch?v=abcDEF123_-"
            );
        }
        for url in [
            "https://www.youtube.com/watch?v=invalid",
            "https://youtube.com.example.org/watch?v=abcDEF123_-",
        ] {
            assert!(youtube_oembed_url(&Url::parse(url).unwrap()).is_none());
        }
    }

    #[test]
    fn reads_open_graph_and_resolves_preview_against_final_url() {
        let url = Url::parse("https://example.org/articles/page").unwrap();
        let parsed = parse_page(
            &url,
            br#"
          <title>Fallback</title>
          <meta property="OG:TITLE" content="  Title &amp; subtitle ">
          <meta name="description" content="Description">
          <meta property="og:image:secure_url" content="">
          <meta property="og:image" content="../cover.png">
          <meta name="twitter:image" content="https://cdn.example.org/fallback.png">
        "#,
            None,
        );
        assert_eq!(parsed.title.as_deref(), Some("Title & subtitle"));
        assert_eq!(parsed.description.as_deref(), Some("Description"));
        assert_eq!(
            parsed.image_url.unwrap().as_str(),
            "https://example.org/cover.png"
        );
    }

    #[test]
    fn reads_title_in_legacy_japanese_encoding() {
        let url = Url::parse("https://example.jp/").unwrap();
        let bytes = encoding_rs::SHIFT_JIS
            .encode("<meta charset=Shift_JIS><title>日本語のタイトル</title>")
            .0;
        let parsed = parse_page(&url, &bytes, Some("text/html"));
        assert_eq!(parsed.title.as_deref(), Some("日本語のタイトル"));
        assert!(parsed.image_url.is_none());
    }

    #[test]
    fn generates_a_bounded_jpeg_for_storage() {
        let mut input = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(800, 400))
            .write_to(&mut input, ImageFormat::Png)
            .unwrap();
        let thumbnail = make_thumbnail(input.get_ref()).unwrap();
        let bytes = decode_thumbnail(&thumbnail).unwrap();
        let dimensions = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
            .into_dimensions()
            .unwrap();
        assert_eq!(dimensions, (640, 320));
        assert!(make_thumbnail(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
        assert!(decode_thumbnail("data:image/jpeg;base64,invalid").is_err());
    }

    #[test]
    fn accepts_only_public_web_destinations_for_lookup() {
        for ip in [
            "127.0.0.1",
            "10.1.0.1",
            "100.64.0.1",
            "169.254.169.254",
            "::1",
            "fd00::1",
            "::ffff:192.168.0.1",
            "2002:7f00:1::1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700::1111".parse().unwrap()));
        assert!(!is_web_url(
            &Url::parse("https://user:pass@example.com").unwrap()
        ));
    }
}
