//! Bilibili: turning a video page into a file that can be downloaded.
//!
//! There is no file behind the page. A bilibili watch page plays through MSE: the player
//! fetches a manifest, then range-requests DASH segments (`*_da2-1-*.m4s`), one stream for
//! video and another for audio, and assembles them in javascript. The `<video>` element
//! therefore has a `blob:` address, and fetching that address from outside the page returns
//! nothing at all - which is why the save button on the video bar refuses there.
//!
//! The way through is the site's own playurl endpoint. It answers with either a DASH pair or,
//! when asked as `platform=html5`, a single mp4 that already contains both tracks. The single
//! file is what this file asks for, deliberately: muxing a DASH pair would mean shipping or
//! calling an ffmpeg this project does not bundle, and a video without sound is not a video.
//!
//! What the endpoint will answer depends on who is asking. Without a session the `html5`
//! platform is capped at 720P, and some videos (member-only, paid, region-locked) answer
//! nothing useful at all. The cookies the tab already holds are passed along for that
//! reason: a reader who paid for a 1080P membership should get 1080P, and the browser is the
//! only thing that knows they did.

use std::path::{Path, PathBuf};

/// What one part of a video is called, for naming the file.
///
/// The `cid` is deliberately not here: it identifies the part on the wire and is read
/// straight out of the view answer by the caller, so carrying it in this struct would be a
/// field nothing reads.
pub struct Part {
    pub title: String,
}

/// The address of one playable file, and how big it is.
#[derive(Debug, Clone)]
pub struct Playable {
    pub url: String,
    pub size: u64,
    /// What the site calls this quality, for the line the reader sees.
    pub quality: String,
}

/// Reads the bvid and the part out of a watch page address.
///
/// Both spellings are accepted because both are in use: `video/BV1xx411c7mD` is what a
/// search result links to, and `video/av12345` is what an old link or the API's own
/// `aid` points at. The `p` query parameter is the part, which is absent for a
/// single-part video.
pub fn parse(url: &str) -> Option<(String, Option<u64>, Option<u64>)> {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("bilibili.com/video/") {
        return None;
    }
    let rest = &url[lower.find("/video/")? + "/video/".len()..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let id = &rest[..end];
    let page = query_value(&rest[end..], "p").and_then(|value| value.parse::<u64>().ok());

    let id_lower = id.to_ascii_lowercase();
    if id_lower.starts_with("bv") {
        // `BV` is case sensitive on the wire; the prefix is not, so the original is kept.
        let original = &id[2..];
        if original.is_empty() {
            return None;
        }
        return Some((format!("BV{original}"), None, page));
    }
    if let Some(av) = id_lower.strip_prefix("av") {
        let aid = av.parse::<u64>().ok()?;
        return Some((String::new(), Some(aid), page));
    }
    None
}

/// One query parameter out of a `?a=b&c=d` tail.
fn query_value(tail: &str, name: &str) -> Option<String> {
    let query = tail.split_once('?')?.1.split('#').next().unwrap_or("");
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

/// A file name that says what the video is called without saying it in path syntax.
///
/// The title is the reader's own language, so this keeps it whole; `downloads::target_for`
/// is what removes the characters a file name may not contain.
///
/// The part is appended only when it adds something. A single-part video usually names its
/// one part after the video itself, and a video whose title already contains the part name
/// would otherwise be saved as "X - X" - measured once on a music video whose title was
/// "【官方 MV】Never Gonna..." and whose only part was "Never Gonna...", which produced a
/// file name that said the same thing twice.
pub fn file_name(title: &str, part: Option<&Part>, index: usize) -> String {
    let base = title.trim();
    let base = if base.is_empty() { "bilibili" } else { base };
    let piece = part.map(|part| part.title.trim()).unwrap_or_default();
    let adds_something = !piece.is_empty() && piece != base && !base.contains(piece);
    match () {
        _ if adds_something => format!("{base} - {piece}.mp4"),
        // The part number keeps a multi-part video's files from overwriting each other when
        // the parts share a title, which they often do.
        _ if index > 0 => format!("{base} - P{}.mp4", index + 1),
        _ => format!("{base}.mp4"),
    }
}

/// Picks the stream to download out of a playurl answer.
///
/// The endpoint answers in two shapes and both are handled here, because which one comes back
/// depends on the platform the caller claimed rather than on anything in the request we care
/// about: `durl` is the single-file answer, `dash` is the split one.
///
/// For `dash` only the video track's address can be returned - it has no sound. This is
/// reported as an error rather than silently saved, because a file that plays silently is
/// exactly the outcome a reader would call broken, and nothing here can mux it.
pub fn pick(json: &serde_json::Value) -> Result<Playable, String> {
    let code = json
        .get("code")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(-1);
    if code != 0 {
        let message = json
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the site refused");
        // -404 is "no such video", -403 is "this account may not watch it"; both are worth
        // saying in the reader's own terms rather than as a number.
        return Err(match code {
            -404 => format!("the site does not know this video ({message})"),
            -403 => format!("this video is not available to this account ({message})"),
            _ => format!("the site answered {code}: {message}"),
        });
    }
    let data = json
        .get("data")
        .ok_or_else(|| "the site sent no play information".to_string())?;

    // The single-file answer first: it is the one that has sound.
    if let Some(first) = data
        .get("durl")
        .and_then(serde_json::Value::as_array)
        .and_then(|list| list.first())
    {
        let url = first
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if url.is_empty() {
            return Err("the site sent an empty address".to_string());
        }
        let size = first
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        return Ok(Playable {
            url: url.to_string(),
            size,
            quality: quality_of(data),
        });
    }

    // The split answer. Its video track is a real address and would download, but silently.
    if data
        .get("dash")
        .and_then(|dash| dash.get("video"))
        .and_then(serde_json::Value::as_array)
        .and_then(|list| list.first())
        .is_some()
    {
        return Err(
            "this video is only offered as separate picture and sound, which this build \
             cannot join into one file"
                .to_string(),
        );
    }

    Err("the site sent no address to download".to_string())
}

/// The label for the quality that was actually granted, e.g. `720P 准高清`.
fn quality_of(data: &serde_json::Value) -> String {
    let granted = data.get("quality").and_then(serde_json::Value::as_u64);
    let Some(granted) = granted else {
        return String::new();
    };
    data.get("support_formats")
        .and_then(serde_json::Value::as_array)
        .and_then(|list| {
            list.iter().find(|entry| {
                entry.get("quality").and_then(serde_json::Value::as_u64) == Some(granted)
            })
        })
        .and_then(|entry| entry.get("new_description"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Where the file goes: the downloads folder, under the name derived from the title.
pub fn destination(downloads: &Path, name: &str) -> PathBuf {
    // Through `target_for` so a name that already exists gets " (1)" rather than being
    // overwritten, and so the characters a file name may not contain are removed.
    crate::downloads::target_for(downloads, "https://www.bilibili.com/video", name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_watch_page_names_its_video_and_its_part() {
        assert_eq!(
            parse("https://www.bilibili.com/video/BV1GJ411x7h7"),
            Some(("BV1GJ411x7h7".to_string(), None, None))
        );
        assert_eq!(
            parse("https://www.bilibili.com/video/BV1GJ411x7h7?p=3"),
            Some(("BV1GJ411x7h7".to_string(), None, Some(3)))
        );
        // A trailing slash and a fragment are both plausible, and neither is part of the id.
        assert_eq!(
            parse("https://www.bilibili.com/video/BV1GJ411x7h7/?spm_id_from=333"),
            Some(("BV1GJ411x7h7".to_string(), None, None))
        );
        // The old spelling, which is what the API's own links use.
        assert_eq!(
            parse("https://www.bilibili.com/video/av80433022"),
            Some((String::new(), Some(80433022), None))
        );
    }

    #[test]
    fn the_case_of_a_bv_id_survives() {
        // `BV` ids are case sensitive: lowercasing one addresses a different video that
        // usually does not exist. Only the two-letter prefix is matched without case.
        let parsed = parse("https://www.bilibili.com/video/bv1GJ411x7h7").expect("parsed");
        assert_eq!(parsed.0, "BV1GJ411x7h7");
    }

    #[test]
    fn a_page_that_is_not_a_watch_page_is_refused() {
        for url in [
            "https://www.bilibili.com/",
            "https://space.bilibili.com/1",
            "https://www.bilibili.com/video/",
            "https://www.bilibili.com/video/xx123",
            "https://example.com/video/BV1GJ411x7h7",
        ] {
            assert!(parse(url).is_none(), "should not parse: {url}");
        }
    }

    #[test]
    fn a_single_file_answer_is_taken_as_it_stands() {
        let answer = json!({
            "code": 0,
            "data": {
                "quality": 64,
                "durl": [{ "url": "https://cdn.example/a.mp4", "size": 51973319 }],
                "support_formats": [
                    { "quality": 16, "new_description": "360P" },
                    { "quality": 64, "new_description": "720P" }
                ]
            }
        });
        let picked = pick(&answer).expect("one file");
        assert_eq!(picked.url, "https://cdn.example/a.mp4");
        assert_eq!(picked.size, 51973319);
        // The quality that was granted, not the best one on the list.
        assert_eq!(picked.quality, "720P");
    }

    #[test]
    fn a_split_answer_is_refused_rather_than_saved_without_sound() {
        // Taking the video track here was the tempting shortcut, and it produces a file that
        // plays with no sound - worse than saying so, because the reader only finds out
        // after the download.
        let answer = json!({
            "code": 0,
            "data": {
                "dash": {
                    "video": [{ "baseUrl": "https://cdn.example/v.m4s" }],
                    "audio": [{ "baseUrl": "https://cdn.example/a.m4s" }]
                }
            }
        });
        let refused = pick(&answer).expect_err("must refuse");
        assert!(refused.contains("separate"), "got: {refused}");
    }

    #[test]
    fn a_refusal_from_the_site_is_said_in_the_reader_s_terms() {
        let missing = json!({ "code": -404, "message": "nothing here" });
        assert!(pick(&missing)
            .expect_err("refused")
            .contains("does not know this video"));

        let forbidden = json!({ "code": -403, "message": "no permission" });
        assert!(pick(&forbidden)
            .expect_err("refused")
            .contains("not available to this account"));

        assert!(pick(&json!({ "code": 0, "data": {} })).is_err());
    }

    #[test]
    fn one_file_answers_do_not_collide_when_they_share_a_title() {
        let part = Part {
            title: "第一话".to_string(),
        };
        let named = file_name("某番", Some(&part), 0);
        assert_eq!(named, "某番 - 第一话.mp4");

        // Parts whose titles match the video title fall back to the part number.
        let same = Part {
            title: "某番".to_string(),
        };
        assert_eq!(file_name("某番", Some(&same), 2), "某番 - P3.mp4");
        assert_eq!(file_name("某番", None, 0), "某番.mp4");
        // An empty title still produces a name.
        assert_eq!(file_name("   ", None, 0), "bilibili.mp4");

        // The regression: a music video whose one part is named after the video itself had
        // its title printed twice, "X - X.mp4", which is what the first version produced.
        let echo = Part {
            title: "【官方 MV】Never Gonna Give You Up".to_string(),
        };
        assert_eq!(
            file_name(
                "【官方 MV】Never Gonna Give You Up - Rick Astley",
                Some(&echo),
                0
            ),
            "【官方 MV】Never Gonna Give You Up - Rick Astley.mp4"
        );
        // A part name already inside the title is not appended again.
        let inside = Part {
            title: "第一话".to_string(),
        };
        assert_eq!(
            file_name("某番 第一话 完整版", Some(&inside), 0),
            "某番 第一话 完整版.mp4"
        );
    }
}
