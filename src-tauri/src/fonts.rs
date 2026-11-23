//! The fonts this machine can actually draw.
//!
//! The list a settings page offers has to be the names a css `font-family` accepts, which
//! means family names: "Segoe UI", not "Segoe UI Bold". The registry under
//! `Windows NT\CurrentVersion\Fonts` stores one entry per *style*, so it answers with
//! "Arial Bold Italic" next to "Arial" and leaves the caller to work out which of those is
//! a family - and it misses fonts installed for a single user, which is where a downloaded
//! font usually lands. DirectWrite keeps the family and its styles apart and returns both
//! kinds of installation through one interface.

use serde::Serialize;

/// One family, as the settings page shows it in a list.
#[derive(Debug, Clone, Serialize)]
pub struct Family {
    /// The name to put in a `font-family` value.
    pub name: String,
    /// A family whose glyphs are all one advance width: what a code block wants.
    pub monospace: bool,
    /// The same family under its own language's name, when it has one.
    pub local: String,
}

/// Every installed family, ordered so the list reads sensibly.
///
/// Sorted by name, with the monospaced ones marked rather than separated: a reader looking
/// for their code font types into the same field as everyone else.
#[cfg(windows)]
pub fn installed() -> Vec<Family> {
    let mut found = enumerate();
    found.sort_by_key(|family| family.name.to_lowercase());
    found.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    found
}

#[cfg(not(windows))]
pub fn installed() -> Vec<Family> {
    Vec::new()
}

#[cfg(windows)]
fn enumerate() -> Vec<Family> {
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, DWRITE_FACTORY_TYPE_SHARED,
    };

    let mut out = Vec::new();
    unsafe {
        let factory: IDWriteFactory = match DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) {
            Ok(factory) => factory,
            Err(err) => {
                crate::logging::warn(&format!("DirectWrite unavailable: {err}"));
                return out;
            }
        };
        let mut collection: Option<IDWriteFontCollection> = None;
        if let Err(err) = factory.GetSystemFontCollection(&mut collection, false) {
            crate::logging::warn(&format!("cannot read the font collection: {err}"));
            return out;
        }
        let Some(collection) = collection else {
            return out;
        };
        let count = collection.GetFontFamilyCount();
        for index in 0..count {
            let Ok(family) = collection.GetFontFamily(index) else {
                continue;
            };
            let Some(name) = family_name(&family) else {
                continue;
            };
            let local = local_family_name(&family).unwrap_or_default();
            let monospace = is_monospaced(&family);
            out.push(Family {
                name,
                monospace,
                local,
            });
        }
    }
    out
}

/// The family's first name, which is the one for the user's own locale when it has one.
#[cfg(windows)]
unsafe fn family_name(
    family: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily,
) -> Option<String> {
    let strings = family.GetFamilyNames().ok()?;
    let count = strings.GetCount();
    (0..count).find_map(|index| read_string(&strings, index))
}

/// The family's second name, which DirectWrite keeps for the family's own language.
#[cfg(windows)]
unsafe fn local_family_name(
    family: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily,
) -> Option<String> {
    let strings = family.GetFamilyNames().ok()?;
    if strings.GetCount() < 2 {
        return None;
    }
    read_string(&strings, 1).filter(|name| Some(name.as_str()) != family_name(family).as_deref())
}

/// One entry of a DirectWrite string list, as text.
#[cfg(windows)]
unsafe fn read_string(
    strings: &windows::Win32::Graphics::DirectWrite::IDWriteLocalizedStrings,
    index: u32,
) -> Option<String> {
    let length = strings.GetStringLength(index).ok()?;
    if length == 0 {
        return None;
    }
    // `GetStringLength` does not count the terminator, but the buffer has to hold it.
    let mut buffer = vec![0u16; length as usize + 1];
    strings.GetString(index, &mut buffer).ok()?;
    let text = String::from_utf16_lossy(&buffer[..length as usize]);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Whether every glyph in the family has the same advance.
///
/// Asked of the regular face: a family whose bold is fixed-width and whose regular is not
/// does not exist in practice, and the regular one is what a css rule would pick.
#[cfg(windows)]
unsafe fn is_monospaced(family: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily) -> bool {
    use windows::core::Interface;
    use windows::Win32::Graphics::DirectWrite::{
        IDWriteFont1, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_WEIGHT_NORMAL,
    };
    let Ok(font) = family.GetFirstMatchingFont(
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        DWRITE_FONT_STYLE_NORMAL,
    ) else {
        return false;
    };
    match font.cast::<IDWriteFont1>() {
        Ok(font1) => font1.IsMonospacedFont().as_bool(),
        Err(_) => false,
    }
}
