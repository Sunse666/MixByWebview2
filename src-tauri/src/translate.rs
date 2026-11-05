//! Page translation through Microsoft's translator, the same engine Edge uses.
//! No key is needed: the web translator page hands out a short lived token and a
//! session cookie, and the text endpoint accepts one string per request.

use parking_lot::Mutex;
use std::time::{Duration, Instant};

const HOME: &str = "https://cn.bing.com/translator?from=auto-detect&to=zh-Hans";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";
/// A single request is silently truncated past ~970 characters.
const CHUNK: usize = 900;

struct Token {
  key: String,
  token: String,
  ig: String,
  at: Instant,
}

static TOKEN: Mutex<Option<Token>> = Mutex::new(None);
static CLIENT: Mutex<Option<reqwest::Client>> = Mutex::new(None);

fn client() -> Result<reqwest::Client, String> {
  let mut guard = CLIENT.lock();
  if let Some(held) = guard.as_ref() {
    return Ok(held.clone());
  }
  let built = reqwest::Client::builder()
    .cookie_store(true)
    .timeout(Duration::from_secs(20))
    .user_agent(UA)
    .build()
    .map_err(|err| err.to_string())?;
  *guard = Some(built.clone());
  Ok(built)
}

/// Between two markers, the text after the marker up to the next quote.
fn between<'a>(hay: &'a str, marker: &str) -> Option<&'a str> {
  let start = hay.find(marker)? + marker.len();
  let rest = &hay[start..];
  let end = rest.find('"')?;
  Some(&rest[..end])
}

async fn token(client: &reqwest::Client) -> Option<Token> {
  {
    let guard = TOKEN.lock();
    if let Some(held) = guard.as_ref() {
      if held.at.elapsed() < Duration::from_secs(600) {
        return Some(Token { key: held.key.clone(), token: held.token.clone(), ig: held.ig.clone(), at: held.at });
      }
    }
  }

  let body = client.get(HOME).send().await.ok()?.text().await.ok()?;
  let key = between(&body, "params_AbusePreventionHelper = [")?;
  let key = key.trim_end_matches(',').to_string();

  let token_marker = format!("params_AbusePreventionHelper = [{key},\"");
  let token = between(&body, &token_marker)?.to_string();
  let ig = between(&body, "IG:\"")?.to_string();

  if key.is_empty() || token.is_empty() || ig.is_empty() {
    return None;
  }
  let fresh = Token { key, token, ig, at: Instant::now() };
  *TOKEN.lock() = Some(Token { key: fresh.key.clone(), token: fresh.token.clone(), ig: fresh.ig.clone(), at: fresh.at });
  Some(fresh)
}

/// Chunks a list of strings into joined batches that respect the request limit.
fn batches(texts: &[String]) -> Vec<Vec<usize>> {
  let mut out = Vec::new();
  let mut current = Vec::new();
  let mut size = 0;

  for (index, text) in texts.iter().enumerate() {
    let len = text.chars().count() + 1;
    if len > CHUNK {
      // Too big on its own: translate a prefix so something shows up.
      let clipped: String = text.chars().take(CHUNK).collect();
      if !current.is_empty() {
        out.push(std::mem::take(&mut current));
        size = 0;
      }
      out.push(vec![index]);
      let _ = clipped;
      continue;
    }
    if size + len > CHUNK && !current.is_empty() {
      out.push(std::mem::take(&mut current));
      size = 0;
    }
    size += len;
    current.push(index);
  }
  if !current.is_empty() {
    out.push(current);
  }
  out
}

async fn translate_chunk(
  client: &reqwest::Client,
  token: &Token,
  joined: &str,
  to: &str,
) -> Result<String, String> {
  let url = format!(
    "https://cn.bing.com/ttranslatev3?isVertical=1&&IG={}&IID=translator.5028.1",
    token.ig
  );
  let response = client
    .post(&url)
    .header("Referer", "https://cn.bing.com/translator")
    .header("Origin", "https://cn.bing.com")
    .form(&[
      ("fromLang", "auto-detect"),
      ("text", joined),
      ("to", to),
      ("token", token.token.as_str()),
      ("key", token.key.as_str()),
    ])
    .send()
    .await
    .map_err(|err| err.to_string())?;

  let body: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
  body
    .get(0)
    .and_then(|item| item.get("translations"))
    .and_then(|list| list.get(0))
    .and_then(|first| first.get("text"))
    .and_then(|text| text.as_str())
    .map(str::to_string)
    .ok_or_else(|| format!("unexpected answer: {body}"))
}

/// Warms the token and the cookie jar so the first click is not slow.
pub async fn warm() {
  if let Ok(client) = client() {
    let _ = token(&client).await;
  }
}

/// One request per node, all at once: fewer round trips than batching.
async fn translate_many(
  client: &reqwest::Client,
  base: &Token,
  texts: &[String],
  to: &str,
) -> Vec<String> {
  let mut handles = Vec::new();
  for (index, text) in texts.iter().enumerate() {
    let client = client.clone();
    let token = Token {
      key: base.key.clone(),
      token: base.token.clone(),
      ig: base.ig.clone(),
      at: base.at,
    };
    let body = text.clone();
    let target = to.to_string();
    handles.push((
      index,
      tauri::async_runtime::spawn(async move { translate_chunk(&client, &token, &body, &target).await }),
    ));
  }

  let mut out = vec![String::new(); texts.len()];
  for (index, handle) in handles {
    if let Ok(Ok(one)) = handle.await {
      out[index] = one;
    }
  }
  out
}

/// Translates one string per slot; empty results mean "leave it alone".
/// The Microsoft engine comes first because it auto-detects the language; the
/// keyless fallback only takes over when the first engine returns nothing.
pub async fn translate(texts: Vec<String>, to: &str) -> Vec<String> {
  let wanted = texts.len();
  let primary = translate_microsoft(texts.clone(), to).await;
  let done = primary.iter().filter(|text| !text.is_empty()).count();
  if done > 0 || wanted == 0 {
    return primary;
  }

  crate::logging::warn("translator: falling back to the secondary engine");
  let secondary = translate_fallback(texts, to).await;
  let recovered = secondary.iter().filter(|text| !text.is_empty()).count();
  crate::logging::info(&format!("translator: fallback recovered {recovered}/{wanted}"));
  secondary
}

async fn translate_microsoft(texts: Vec<String>, to: &str) -> Vec<String> {
  let mut out = vec![String::new(); texts.len()];
  let Ok(client) = client() else {
    return out;
  };
  let Some(first) = token(&client).await else {
    crate::logging::warn("translator: no token");
    return out;
  };

  // Short pages: parallel single requests beat batching.
  if texts.len() <= 12 {
    return translate_many(&client, &first, &texts, to).await;
  }

  for group in batches(&texts) {
    // A single long string goes on its own, truncated to the request limit.
    if group.len() == 1 {
      let only = &texts[group[0]];
      let clipped: String = only.chars().take(CHUNK).collect();
      if let Ok(text) = translate_chunk(&client, &first, &clipped, to).await {
        out[group[0]] = text;
      }
      continue;
    }

    let joined = group
      .iter()
      .map(|index| texts[*index].as_str())
      .collect::<Vec<_>>()
      .join("\n");

    match translate_chunk(&client, &first, &joined, to).await {
      Ok(translated) => {
        let parts: Vec<&str> = translated.split('\n').collect();
        if parts.len() == group.len() {
          for (slot, part) in group.iter().zip(parts) {
            out[*slot] = part.trim().to_string();
          }
        } else {
          // Line structure changed; fall back to one request per string.
          for index in group {
            let text = &texts[index];
            if let Ok(one) = translate_chunk(&client, &first, text, to).await {
              out[index] = one;
            }
          }
        }
      }
      Err(err) => crate::logging::warn(&format!("translator chunk failed: {err}")),
    }
  }

  out
}

/// Script based language guess; MyMemory refuses an "auto" source language.
fn guess_source(text: &str) -> Option<&'static str> {
  let mut latin = 0;
  for ch in text.chars() {
    match ch {
      '\u{4e00}'..='\u{9fff}' => return None,
      '\u{0400}'..='\u{04FF}' => return Some("ru"),
      '\u{0600}'..='\u{06FF}' => return Some("ar"),
      '\u{3040}'..='\u{30FF}' => return Some("ja"),
      '\u{AC00}'..='\u{D7AF}' => return Some("ko"),
      _ => {
        if ch.is_ascii_alphabetic() {
          latin += 1;
        }
      }
    }
  }
  if latin == 0 {
    return None;
  }
  Some("en")
}

fn fallback_target(to: &str) -> &str {
  if to.starts_with("zh") {
    "zh-CN"
  } else {
    to
  }
}

async fn translate_fallback(texts: Vec<String>, to: &str) -> Vec<String> {
  let mut out = vec![String::new(); texts.len()];
  let Ok(client) = client() else {
    return out;
  };
  let target = fallback_target(to);
  let mut budget = 3200usize;

  for (index, text) in texts.iter().enumerate() {
    let trimmed = text.trim();
    if trimmed.len() < 2 || budget < trimmed.len() {
      continue;
    }
    let Some(source) = guess_source(trimmed) else {
      continue;
    };
    budget -= trimmed.len();

    let url = format!(
      "https://api.mymemory.translated.net/get?q={}&langpair={source}|{target}",
      crate::permissions::encode(trimmed)
    );
    let Ok(response) = client.get(&url).send().await else {
      continue;
    };
    let body: serde_json::Value = response.json().await.unwrap_or_default();
    let translated = body
      .get("responseData")
      .and_then(|data| data.get("translatedText"))
      .and_then(|value| value.as_str())
      .unwrap_or_default()
      .to_string();
    if translated.contains("MYMEMORY WARNING") {
      crate::logging::warn("translator: secondary engine quota reached");
      break;
    }
    if !translated.is_empty() && translated != trimmed {
      out[index] = translated;
    }
  }

  out
}
