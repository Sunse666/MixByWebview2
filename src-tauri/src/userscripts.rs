use serde::Serialize;
use std::path::Path;

/// One userscript after header parsing.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptInfo {
    pub name: String,
    pub run_at: String,
    pub matches: Vec<String>,
    pub path: String,
    /// Disabled scripts are listed but never injected.
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone)]
pub struct Userscript {
    pub name: String,
    pub matches: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// "start" or "end"
    pub run_at: String,
    pub code: String,
    pub path: String,
}

impl Userscript {
    pub fn info(&self) -> ScriptInfo {
        ScriptInfo {
            name: self.name.clone(),
            run_at: self.run_at.clone(),
            matches: self.matches.clone(),
            path: self.path.clone(),
            disabled: self.path.ends_with(".user.js.disabled"),
        }
    }
}

pub fn scan(dir: &Path) -> Vec<Userscript> {
    let mut found = Vec::new();
    collect(dir, 0, &mut found);
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<Userscript>) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let is_script = file_name.ends_with(".user.js") || file_name.ends_with(".user.js.disabled");
        if !is_script {
            continue;
        }

        match std::fs::read_to_string(&path) {
            Ok(source) => match parse(&source) {
                Some(mut script) => {
                    script.path = path.display().to_string();
                    if file_name.ends_with(".user.js.disabled") {
                        crate::logging::info(&format!("userscript {} (disabled)", script.name));
                        out.push(script);
                        continue;
                    }
                    crate::logging::info(&format!("userscript {} ({})", script.name, script.run_at));
                    out.push(script);
                }
                None => crate::logging::warn(&format!("userscript without a header: {}", path.display())),
            },
            Err(err) => crate::logging::warn(&format!("cannot read {}: {err}", path.display())),
        }
    }
}

/// Reads the "// ==UserScript==" block. Unknown keys are ignored on purpose.
pub fn parse(source: &str) -> Option<Userscript> {
    let mut in_header = false;
    let mut saw_header = false;
    let mut name = String::new();
    let mut matches = Vec::new();
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut run_at = "end".to_string();

    for line in source.lines().take(200) {
        let trimmed = line.trim();
        if trimmed.starts_with("// ==UserScript==") {
            in_header = true;
            saw_header = true;
            continue;
        }
        if trimmed.starts_with("// ==/UserScript==") {
            break;
        }
        if !in_header {
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("//") else {
            continue;
        };
        let rest = rest.trim();
        let Some((key, value)) = rest.split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        match key {
            "@name" => name = value.to_string(),
            "@match" => matches.push(value.to_string()),
            "@include" => includes.push(value.to_string()),
            "@exclude" => excludes.push(value.to_string()),
            "@run-at" => {
                run_at = match value {
                    "document-start" => "start".to_string(),
                    _ => "end".to_string(),
                }
            }
            _ => {}
        }
    }

    if !saw_header || (matches.is_empty() && includes.is_empty()) {
        return None;
    }

    Some(Userscript {
        name: if name.is_empty() {
            "userscript".to_string()
        } else {
            name
        },
        matches,
        includes,
        excludes,
        run_at,
        code: source.to_string(),
        path: String::new(),
    })
}

/// Compiles @match / @include patterns into regular expressions tested in the page.
pub fn compiled_patterns(script: &Userscript) -> (Vec<String>, Vec<String>) {
    let mut allow = Vec::new();
    for pattern in script.matches.iter() {
        if let Some(regex) = match_to_regex(pattern) {
            allow.push(regex);
        }
    }
    for pattern in script.includes.iter() {
        allow.push(glob_to_regex(pattern));
    }
    let deny = script.excludes.iter().map(|p| glob_to_regex(p)).collect();
    (allow, deny)
}

pub fn match_to_regex(pattern: &str) -> Option<String> {
    let pattern = pattern.trim();
    if pattern == "<all_urls>" {
        return Some(r"^(?:https?|file|ftp)://".to_string());
    }

    let (scheme, rest) = pattern.split_once("://")?;
    let scheme_regex = match scheme {
        "*" => "(?:https?)".to_string(),
        other => regex::escape_component(other),
    };

    let (host, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/*"),
    };

    let host_regex = if host == "*" {
        "[^/]+".to_string()
    } else if let Some(domain) = host.strip_prefix("*.") {
        let dot = r"\.";
        format!("(?:[^/]+{dot})?{}", regex::escape_component(domain))
    } else {
        regex::escape_component(host)
    };

    Some(format!(
        "^{scheme_regex}://{host_regex}{}$",
        glob_fragment(path)
    ))
}

/// Anchored version, used for @include / @exclude.
pub fn glob_to_regex(pattern: &str) -> String {
    format!("^{}$", glob_fragment(pattern))
}

/// Unanchored fragment, so @match can stitch scheme + host + path together.
pub fn glob_fragment(pattern: &str) -> String {
    let mut out = String::new();
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '.' => out.push_str("\\."),
            '/' => out.push('/'),
            '(' | ')' | '[' | ']' | '{' | '}' | '+' | '?' | '|' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

mod regex {
    pub fn escape_component(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for ch in input.chars() {
            if matches!(
                ch,
                '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\'
            ) {
                out.push('\\');
            }
            out.push(ch);
        }
        out
    }
}

/// A single init script that carries every userscript and matches them in page.
pub fn loader_js(scripts: &[Userscript]) -> String {
    if scripts.is_empty() {
        return String::new();
    }

    let entries: Vec<serde_json::Value> = scripts
        .iter()
        .map(|script| {
            let (allow, deny) = compiled_patterns(script);
            serde_json::json!({
                "name": script.name,
                "allow": allow,
                "deny": deny,
                "start": script.run_at == "start",
                "code": script.code,
            })
        })
        .collect();

    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());

    format!(
        r#"
(function () {{
  if (window.__mixUS) return;
  window.__mixUS = true;

  var SCRIPTS = {json};
  var href = location.href;

  var hits = function (script) {{
    var allow = script.allow.some(function (source) {{ return new RegExp(source).test(href); }});
    if (!allow) return false;
    return !script.deny.some(function (source) {{ return new RegExp(source).test(href); }});
  }};

  // Minimal GM_* subset backed by prefixed localStorage.
  var gm = function (name) {{
    var prefix = "__mix_gm__" + name + "__";
    return {{
      get: function (key, fallback) {{
        var raw = localStorage.getItem(prefix + key);
        if (raw === null) return fallback;
        try {{ return JSON.parse(raw); }} catch (error) {{ return fallback; }}
      }},
      set: function (key, value) {{
        try {{ localStorage.setItem(prefix + key, JSON.stringify(value)); }} catch (error) {{}}
      }}
    }};
  }};

  var runOne = function (script) {{
    var store = gm(script.name);
    var sandbox = {{
      GM_getValue: store.get,
      GM_setValue: store.set,
      GM_info: {{ script: {{ name: script.name }} }},
      unsafeWindow: window
    }};
    try {{
      new Function("GM_getValue", "GM_setValue", "GM_info", "unsafeWindow", script.code)(
        sandbox.GM_getValue, sandbox.GM_setValue, sandbox.GM_info, sandbox.unsafeWindow
      );
    }} catch (error) {{
      console.warn("[mix] userscript failed:", script.name, error);
    }}
  }};

  var run = function (onlyStart) {{
    SCRIPTS.forEach(function (script) {{
      if (!!script.start !== onlyStart) return;
      if (hits(script)) runOne(script);
    }});
  }};

  run(true);
  if (document.readyState === "loading") {{
    document.addEventListener("DOMContentLoaded", function () {{ run(false); }});
  }} else {{
    run(false);
  }}
}})();
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "// ==UserScript==\n// @name  Demo\n// @match    https://example.com/*\n// @run-at  document-start\n// ==/UserScript==\nconsole.log(1);\n";

    #[test]
    fn parses_the_header() {
        let script = parse(SCRIPT).expect("parse");
        assert_eq!(script.name, "Demo");
        assert_eq!(script.run_at, "start");
        assert_eq!(script.matches, vec!["https://example.com/*"]);
    }

    #[test]
    fn rejects_scripts_without_patterns() {
        assert!(parse("// ==UserScript==\n// @name x\n// ==/UserScript==\n").is_none());
        assert!(parse("console.log(1)").is_none());
    }

    #[test]
    fn all_urls_matches_any_scheme() {
        let regex = match_to_regex("<all_urls>").expect("regex");
        assert!(regex.starts_with("^(?:https?|file|ftp)://"));
    }

    #[test]
    fn wildcard_subdomain_covers_the_bare_domain() {
        let regex = match_to_regex("*://*.example.com/*").expect("regex");
        assert!(regex.contains("(?:[^/]+\\.)?example\\.com"), "got {regex}");
    }

    #[test]
    fn patterns_are_anchored() {
        let regex = match_to_regex("https://example.com/path/*").expect("regex");
        assert!(regex.starts_with('^') && regex.ends_with('$'));
    }

    #[test]
    fn match_regex_has_no_inner_anchors() {
        let fragment = glob_fragment("/*");
        assert!(!fragment.contains('^') && !fragment.contains('$'));

        assert_eq!(match_to_regex("*://*/*").unwrap(), "^(?:https?)://[^/]+/.*$");
        let site = match_to_regex("https://example.com/docs/*").unwrap();
        assert!(site.starts_with("^https://example"), "got {site}");
        assert!(site.ends_with("/docs/.*$"), "got {site}");
    }
    #[test]
    fn loader_is_empty_without_scripts() {
        assert!(loader_js(&[]).is_empty());
    }
}