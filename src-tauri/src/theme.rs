use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 2;

/// One token per surface the UI paints. Empty strings mean "derive it",
/// which keeps hand written theme files short.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub bg: String,
    pub chrome: String,
    pub accent: String,
    pub text: String,
    pub border: String,
    /// Background of the selected tab pill.
    pub tab: String,
    /// Text colour inside the selected tab pill.
    pub tab_text: String,
    /// Address bar field.
    pub url_bg: String,
    /// Hover wash for rows, tabs and buttons.
    pub hover: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub schema_version: u32,
    pub colors: Colors,
    pub radius: f64,
    pub tab_radius: f64,
    pub font_size: f64,
    pub font_family: String,
    pub opacity: f64,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            bg: "#1F2020".into(),
            chrome: "#1F2020".into(),
            accent: "#00FF00".into(),
            text: "#E8E8E8".into(),
            border: "#2E3131".into(),
            tab: String::new(),
            tab_text: String::new(),
            url_bg: String::new(),
            hover: String::new(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            colors: Colors::default(),
            radius: 15.0,
            tab_radius: 8.0,
            font_size: 13.0,
            font_family: String::new(),
            opacity: 1.0,
        }
    }
}

/// Blends two hex colours; `t` is how much of `b` ends up in the result.
fn mix(a: &str, b: &str, t: f64) -> String {
    let (Some(a), Some(b)) = (parse_hex(a), parse_hex(b)) else {
        return a.to_string();
    };
    let blend = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", blend(a.0, b.0), blend(a.1, b.1), blend(a.2, b.2))
}

fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

impl Theme {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default().resolved();
        };
        match serde_json::from_str::<Theme>(&raw) {
            Ok(theme) => theme.sanitized(),
            Err(err) => {
                crate::logging::warn(&format!("theme.json unreadable ({err}); backing up"));
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default().resolved()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        crate::paths::write_atomic(path, json.as_bytes())
    }

    /// Fills the derived tokens so the UI never sees an empty colour.
    pub fn resolved(mut self) -> Self {
        let base = self.colors.clone();
        if self.colors.tab.is_empty() {
            self.colors.tab = mix(&base.chrome, &base.accent, 0.16);
        }
        if self.colors.tab_text.is_empty() {
            self.colors.tab_text = base.text.clone();
        }
        if self.colors.url_bg.is_empty() {
            self.colors.url_bg = mix(&base.bg, "#000000", 0.25);
        }
        if self.colors.hover.is_empty() {
            self.colors.hover = mix(&base.chrome, &base.text, 0.12);
        }
        if self.font_family.trim().is_empty() {
            self.font_family = "Segoe UI, system-ui, sans-serif".into();
        }
        self
    }

    /// Keep a hand-edited or half-typed value from making the window unusable.
    pub fn sanitized(mut self) -> Self {
        let fallback = Colors::default();
        for (value, default) in [
            (&mut self.colors.bg, &fallback.bg),
            (&mut self.colors.chrome, &fallback.chrome),
            (&mut self.colors.accent, &fallback.accent),
            (&mut self.colors.text, &fallback.text),
            (&mut self.colors.border, &fallback.border),
        ] {
            if !is_hex_color(value) {
                *value = default.clone();
            }
        }
        // Derived tokens may stay empty; anything else must be a real colour.
        for value in [
            &mut self.colors.tab,
            &mut self.colors.tab_text,
            &mut self.colors.url_bg,
            &mut self.colors.hover,
        ] {
            if !value.is_empty() && !is_hex_color(value) {
                value.clear();
            }
        }
        self.radius = self.radius.clamp(0.0, 32.0);
        self.tab_radius = self.tab_radius.clamp(0.0, 20.0);
        self.font_size = self.font_size.clamp(10.0, 20.0);
        self.opacity = self.opacity.clamp(0.4, 1.0);
        // Refuse anything that could break out of the CSS value.
        if self.font_family.contains(|c: char| c == ';' || c == '{' || c == '}' || c == '"') {
            self.font_family.clear();
        }
        self.schema_version = SCHEMA;
        self.resolved()
    }
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.len(), 4 | 7)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_tokens_appear_when_missing() {
        let theme = Theme::default().sanitized();
        assert!(is_hex_color(&theme.colors.tab));
        assert!(is_hex_color(&theme.colors.url_bg));
        assert!(is_hex_color(&theme.colors.hover));
        assert!(!theme.font_family.is_empty());
    }

    #[test]
    fn bad_font_family_is_dropped() {
        let mut theme = Theme::default();
        theme.font_family = "x; }".into();
        assert_eq!(theme.sanitized().font_family, "Segoe UI, system-ui, sans-serif");
    }

    #[test]
    fn numbers_are_clamped() {
        let mut theme = Theme::default();
        theme.radius = 999.0;
        theme.tab_radius = -4.0;
        theme.font_size = 3.0;
        let theme = theme.sanitized();
        assert_eq!(theme.radius, 32.0);
        assert_eq!(theme.tab_radius, 0.0);
        assert_eq!(theme.font_size, 10.0);
    }

    #[test]
    fn mix_blends_channels() {
        assert_eq!(mix("#000000", "#FFFFFF", 0.5), "#808080");
        assert_eq!(mix("nope", "#FFFFFF", 0.5), "nope");
    }

    #[test]
    fn old_schema_still_loads() {
        let dir = std::env::temp_dir().join("mix-theme-old");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.json");
        let legacy = concat!(
            "{\"schema_version\":1,\"colors\":{\"bg\":\"#101010\",",
            "\"chrome\":\"#202020\",\"accent\":\"#00FF00\",",
            "\"text\":\"#EEEEEE\",\"border\":\"#303030\"},",
            "\"radius\":10,\"font_size\":14,\"opacity\":1}"
        );
        std::fs::write(&path, legacy).unwrap();
        let theme = Theme::load(&path);
        assert_eq!(theme.colors.bg, "#101010");
        assert!(is_hex_color(&theme.colors.hover));
    }
}
