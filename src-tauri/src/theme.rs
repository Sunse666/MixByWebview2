use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub schema_version: u32,
    pub colors: Colors,
    pub radius: f64,
    pub font_size: f64,
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub bg: String,
    pub chrome: String,
    pub accent: String,
    pub text: String,
    pub border: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            bg: "#1F2020".into(),
            chrome: "#1F2020".into(),
            accent: "#00FF00".into(),
            text: "#E8E8E8".into(),
            border: "#2E3131".into(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            colors: Colors::default(),
            radius: 15.0,
            font_size: 13.0,
            opacity: 1.0,
        }
    }
}

impl Theme {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Theme>(&raw) {
            Ok(theme) => theme.sanitized(),
            Err(err) => {
                crate::logging::warn(&format!("theme.json unreadable ({err}); backing up"));
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
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
        self.radius = self.radius.clamp(0.0, 32.0);
        self.font_size = self.font_size.clamp(10.0, 20.0);
        self.opacity = self.opacity.clamp(0.4, 1.0);
        self.schema_version = SCHEMA;
        self
    }
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.len(), 4 | 7)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}
