use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped when the on-disk shape changes.
pub const SCHEMA: u32 = 1;

fn default_proxy_mode() -> String {
    "system".to_string()
}

/// The sidebar panels, in the order they are offered. The list is the setting: a panel
/// that is not in it has no button and is not shown.
pub fn default_sidebar_views() -> Vec<String> {
    vec![
        "bookmarks".to_string(),
        "downloads".to_string(),
        "history".to_string(),
        "tabs".to_string(),
    ]
}

fn default_sidebar() -> String {
    "off".to_string()
}

/// For settings that are on unless the user turns them off.
fn default_true() -> bool {
    true
}

fn default_bar_mode() -> String {
    "never".to_string()
}

fn default_bookmark_column() -> f64 {
    200.0
}

fn default_bookmark_button() -> f64 {
    132.0
}

fn default_bookmark_reveal_delay() -> u32 {
    250
}

fn default_bookmark_hide_delay() -> u32 {
    1200
}

fn default_tab_shape() -> String {
    "rounded".to_string()
}

fn default_tab_min_width() -> f64 {
    120.0
}

fn default_tab_scroll_speed() -> u32 {
    3
}

fn default_audio_click() -> String {
    "mute".to_string()
}

fn default_open_foreground() -> String {
    "foreground".to_string()
}

fn default_open_current() -> String {
    "current".to_string()
}

fn default_tab_position() -> String {
    "after_current".to_string()
}

fn default_close_to() -> String {
    "auto".to_string()
}

fn default_last_close() -> String {
    "new_tab".to_string()
}

fn default_middle_tab() -> String {
    "close".to_string()
}

fn default_middle_strip() -> String {
    "new_tab".to_string()
}

/// Keeps a value only when it is one of the answers the row offers.
///
/// Every one of these settings is a `select`, so the set of legal values is fixed and known;
/// a value outside it can only come from a hand-edited file or an older build. Falling back
/// to the default is what keeps the code that reads them to a single `match` with no
/// catch-all branch.
fn one_of(value: &str, allowed: &[&str], fallback: fn() -> String) -> String {
    if allowed.contains(&value) {
        value.to_string()
    } else {
        fallback()
    }
}

fn default_sidebar_width() -> f64 {
    300.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub start_url: String,
    pub restore_session: bool,
    /// Height the chrome page last reported.
    pub chrome_height: f64,
    /// Adds a bookmarks row under the address bar, which also grows the chrome.
    pub show_bookmarks: bool,
    /// How the bar appears: "always", "never" or "address" (while the address bar has the
    /// keyboard). `show_bookmarks` is kept as the "always" answer for files written before
    /// this existed; `sanitize` folds the two together.
    #[serde(default = "default_bar_mode")]
    pub bookmark_bar_mode: String,
    /// Width of one column of the bookmarks menu, and the most a bar button may grow to.
    #[serde(default = "default_bookmark_column")]
    pub bookmark_column_width: f64,
    #[serde(default = "default_bookmark_button")]
    pub bookmark_button_width: f64,
    /// Show the folders of the bookmarks bar as a menu of columns.
    #[serde(default = "default_true")]
    pub bookmark_menu_columns: bool,
    /// Reveal that menu when the pointer reaches this many pixels inside the left edge.
    /// Zero turns the reveal off.
    #[serde(default)]
    pub bookmark_reveal_edge: u32,
    #[serde(default = "default_bookmark_reveal_delay")]
    pub bookmark_reveal_delay: u32,
    #[serde(default = "default_bookmark_hide_delay")]
    pub bookmark_hide_delay: u32,
    /// Put a bookmark added now at the top of its folder instead of the bottom.
    #[serde(default)]
    pub bookmark_newest_first: bool,
    /// Draw icons without their titles on the bookmarks bar.
    #[serde(default)]
    pub bookmark_icons_only: bool,
    /// Show the bar on the new tab page even when it is otherwise hidden.
    #[serde(default)]
    pub bookmark_bar_newtab: bool,
    /// A middle click on a folder opens every bookmark it holds.
    #[serde(default = "default_true")]
    pub bookmark_folder_middle_click: bool,
    /// Proxy: "system", "off" or "manual" (uses proxy_server).
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default)]
    pub proxy_server: String,
    /// Where the sidebar sits: "off", "left" or "right".
    #[serde(default = "default_sidebar")]
    pub sidebar: String,
    /// Sidebar width in logical pixels.
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f64,
    /// Sidebar panels to offer; see default_sidebar_views.
    #[serde(default = "default_sidebar_views")]
    pub sidebar_views: Vec<String>,
    /// Toolbar buttons the user kept, in display order.
    pub toolbar: Vec<String>,
    /// Extensions the user hid from the toolbar, by name.
    pub hidden_extensions: Vec<String>,
    /// Sites that are translated as soon as they load, by host, without `www.`.
    pub translate_hosts: Vec<String>,
    /// Extensions that must not load at all; takes effect on the next start.
    pub disabled_extensions: Vec<String>,
    /// Last state the browser confirmed for an extension, by folder.
    #[serde(default)]
    pub extension_state: std::collections::BTreeMap<String, bool>,

    pub translate_to: String,
    /// A search template with %s; wins over search_engine when set.
    pub custom_search: String,
    /// Right button gestures such as L for back.
    pub gestures: bool,
    /// Drag a link, image or text to act on it.
    pub super_drag: bool,
    /// Show the little badge while dragging.
    pub drag_hint: bool,
    /// Pixels before a drag counts.
    pub drag_threshold: u32,
    /// Direction -> action, e.g. "L" -> "open", "U" -> "copy".
    pub drag_map: std::collections::BTreeMap<String, String>,
    /// Draw the trail line while a gesture is in progress.
    pub gesture_trail: bool,
    /// Trail colour, any css colour.
    pub gesture_color: String,
    /// Trail width in pixels.
    pub gesture_width: u32,
    /// Minimum movement before a direction counts, in pixels.
    pub gesture_threshold: u32,
    /// Pattern -> action, e.g. "UD" -> "reload".
    pub gesture_map: std::collections::BTreeMap<String, String>,
    /// Comma or space separated hosts where gestures are off.
    pub gesture_blocked: String,
    /// dark or light; overrides the theme file colours.
    pub theme_mode: String,
    /// Font family forced on page text; empty keeps the site font.
    pub font_family: String,
    /// Font family forced on code and other monospaced elements.
    pub font_mono: String,
    /// Base font size in pixels, 0 keeps the site size.
    pub font_size: u32,
    /// Smallest size the page may render text at, 0 leaves it alone.
    pub font_min_size: u32,
    /// Opened by the home button; empty falls back to start_url.
    pub home_url: String,
    /// Zoom applied to every page when it opens, 1.0 is 100%.
    pub default_zoom: f64,
    /// Double click on a tab closes it.
    pub tab_double_click_close: bool,
    /// Show the close button on each tab.
    pub tab_show_close: bool,
    /// After closing, activate the tab to the left instead of the right.
    pub tab_close_goes_left: bool,
    /// A right click on a tab closes it; holding Shift still opens the tab menu.
    #[serde(default)]
    pub tab_right_click_close: bool,
    /// A link opened from the new tab page reuses that tab.
    #[serde(default = "default_true")]
    pub tab_reuse_blank: bool,
    /// Every link opens in the tab it was clicked from.
    #[serde(default)]
    pub tab_force_current: bool,
    /// Every link opens in the background of the tab it was clicked from.
    #[serde(default)]
    pub tab_force_background: bool,
    /// A pinned tab cannot be closed by a click or a shortcut, only from its menu.
    #[serde(default)]
    pub tab_pinned_menu_only: bool,
    /// A new tab copies the small-tab state of the one it was opened from.
    #[serde(default = "default_true")]
    pub tab_small_inherit: bool,
    /// The wheel over the strip walks the tabs.
    #[serde(default)]
    pub tab_wheel_switch: bool,
    /// Ask before a command closes more than one tab.
    #[serde(default)]
    pub tab_close_warn: bool,
    /// Tab shape: "rounded" or "square".
    #[serde(default = "default_tab_shape")]
    pub tab_shape: String,
    /// Narrowest a tab may get before the strip starts scrolling.
    #[serde(default = "default_tab_min_width")]
    pub tab_min_width: f64,
    /// Draw the small button that lists every tab.
    #[serde(default = "default_true")]
    pub tab_list_button: bool,
    /// Draw the two scroll arrows beside the strip.
    #[serde(default = "default_true")]
    pub tab_scroll_buttons: bool,
    /// Hover this many milliseconds and the tab activates. Zero turns it off.
    #[serde(default)]
    pub tab_hover_activate: u32,
    /// How many tabs one notch of the wheel moves.
    #[serde(default = "default_tab_scroll_speed")]
    pub tab_scroll_speed: u32,
    /// What the speaker on a tab does: "mute" or "activate".
    #[serde(default = "default_audio_click")]
    pub tab_audio_click: String,
    /// Where a bookmark opens: "foreground", "background" or "current".
    #[serde(default = "default_open_foreground")]
    pub open_bookmark: String,
    #[serde(default = "default_open_foreground")]
    pub open_home: String,
    #[serde(default = "default_open_foreground")]
    pub open_search: String,
    /// An address typed in the bar: "current", "foreground" or "background".
    #[serde(default = "default_open_current")]
    pub open_address: String,
    /// Where a new tab is inserted: "after_current", "after_group" or "at_end".
    #[serde(default = "default_tab_position")]
    pub tab_new_position: String,
    /// Which tab is selected after the current one closes.
    #[serde(default = "default_close_to")]
    pub tab_close_to: String,
    /// What closing the last tab does: "new_tab", "close_window" or "nothing".
    #[serde(default = "default_last_close")]
    pub tab_last_close: String,
    /// A middle click on a tab: "close", "reload" or "nothing".
    #[serde(default = "default_middle_tab")]
    pub tab_middle_click: String,
    /// A middle click on the empty part of the strip: "new_tab", "close_others" or "nothing".
    #[serde(default = "default_middle_strip")]
    pub tab_middle_strip: String,
    /// New tab page: show the search field.
    pub newtab_show_search: bool,
    /// New tab page: show the frequent site tiles.
    pub newtab_show_sites: bool,
    /// Opens instead of the built-in page when set.
    pub newtab_url: String,
    /// Tiles shown at most.
    pub newtab_tiles: u32,
    /// Grid columns for the tiles.
    pub newtab_columns: u32,
    /// Tile corner radius in percent.
    pub newtab_radius: u32,
    /// Action -> combo, e.g. "new_tab" -> "ctrl+t". Empty means the default.
    #[serde(default)]
    pub shortcuts: std::collections::BTreeMap<String, String>,
    /// Engine used when the address bar input is not a url: baidu | bing | google | ddg.
    pub search_engine: String,
    /// New tabs open the built-in page instead of the start url.
    pub new_tab_page: bool,
    /// Interface language: "en" or "zh".
    pub language: String,
    /// Suspend tabs beyond the live limit to give memory back.
    pub memory_saver: bool,
    /// Hosts that are never suspended, comma separated.
    pub keep_alive_hosts: String,
    /// Draw the suspended marker on idle tabs.
    pub mark_suspended: bool,
    /// Show a preview card when the pointer rests on a tab.
    pub tab_hover_preview: bool,
    /// Stop the timers of a background tab that is not playing sound.
    pub freeze_background: bool,
    /// Set by the extensions page: clear the profile's extension state on the next start.
    #[serde(default)]
    pub reset_extensions: bool,
    /// Wake a background tab to photograph it instead of reusing the stored picture.
    pub live_thumbs: bool,
    /// Draw the memory and node count on the tab hover card.
    ///
    /// A second setting rather than one switch for the whole card: the numbers come from a
    /// separate, slower read of the webview's process list, and a reader who wants the
    /// picture rarely wants that read to happen on every hover.
    #[serde(default = "default_true")]
    pub thumb_show_memory: bool,
    pub max_live_tabs: usize,
    pub download_dir: Option<String>,
    /// Draw the tab groups in the bookmarks bar as well as in the strip.
    #[serde(default)]
    pub groups_in_bookmarks: bool,
    /// Pin a group to the bookmarks bar as soon as it is made.
    #[serde(default)]
    pub group_auto_pin: bool,
    /// Where the tab search panel opens: "right", "left" or "center".
    #[serde(default = "default_search_place")]
    pub tab_search_place: String,

    // --- browsing ---------------------------------------------------------
    /// Hold the middle button and move to scroll.
    #[serde(default = "default_true")]
    pub autoscroll: bool,
    /// A middle click pastes the clipboard instead of opening a link in a tab.
    #[serde(default)]
    pub middle_click_paste: bool,
    /// Copy the selection as soon as it is made.
    #[serde(default = "default_true")]
    pub copy_on_select: bool,
    /// Say so when a page goes fullscreen, so a key that leaves it is not a surprise.
    #[serde(default = "default_true")]
    pub fullscreen_hint: bool,
    /// Say so when a video opens in a window of its own.
    #[serde(default = "default_true")]
    pub video_popup_hint: bool,
    /// Put a video's popup window back where it was moved to.
    #[serde(default)]
    pub remember_popup_position: bool,
    /// Draw the whole app at 96 dpi whatever the display reports.
    ///
    /// Read once, before the first window exists: Windows fixes a process's dpi awareness
    /// when it starts, and nothing re-decides it later. Changing this therefore takes a
    /// restart, which the row that sets it says.
    #[serde(default)]
    pub ignore_system_dpi: bool,
    /// Tint every page towards a paper colour.
    #[serde(default)]
    pub eye_comfort: bool,
    /// The tint itself, as `#RRGGBB`.
    #[serde(default = "default_eye_color")]
    pub eye_comfort_color: String,
    /// Schemes a page may hand to the system without being asked.
    #[serde(default = "default_external_schemes")]
    pub external_schemes: String,
}

fn default_eye_color() -> String {
    "#C7EDCC".to_string()
}

fn default_external_schemes() -> String {
    "tg,potplayer,torrent,bc,magnet,thunder,thunderx,ed2k,qqdl,aliim,tencent,baiduyunguanjia,mms,rtsp,acestream,creo"
        .to_string()
}

fn default_search_place() -> String {
    "right".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            start_url: "https://example.com".into(),
            restore_session: true,
            // Zero means "the strip has not measured itself yet"; the strip reports its
            // real height on every start, and this field is only where it lands.
            chrome_height: 0.0,
            show_bookmarks: false,
            bookmark_bar_mode: default_bar_mode(),
            bookmark_column_width: default_bookmark_column(),
            bookmark_button_width: default_bookmark_button(),
            bookmark_menu_columns: true,
            bookmark_reveal_edge: 0,
            bookmark_reveal_delay: default_bookmark_reveal_delay(),
            bookmark_hide_delay: default_bookmark_hide_delay(),
            bookmark_newest_first: false,
            bookmark_icons_only: false,
            bookmark_bar_newtab: false,
            bookmark_folder_middle_click: true,
            proxy_mode: default_proxy_mode(),
            proxy_server: String::new(),
            sidebar: default_sidebar(),
            sidebar_width: default_sidebar_width(),
            sidebar_views: default_sidebar_views(),
            toolbar: vec![
                "translate".into(),
                "zoom".into(),
                "bookmark".into(),
                "extensions".into(),
                "downloads".into(),
            ],
            hidden_extensions: Vec::new(),
            translate_hosts: Vec::new(),
            disabled_extensions: Vec::new(),
            extension_state: std::collections::BTreeMap::new(),
            translate_to: "zh".into(),
            gestures: true,
            super_drag: true,
            drag_hint: true,
            drag_threshold: 28,
            drag_map: std::collections::BTreeMap::new(),
            gesture_trail: true,
            gesture_color: "#4c8dff".into(),
            gesture_width: 3,
            gesture_threshold: 34,
            gesture_map: std::collections::BTreeMap::new(),
            gesture_blocked: String::new(),
            theme_mode: "dark".to_string(),
            font_family: String::new(),
            font_mono: String::new(),
            font_size: 0,
            font_min_size: 0,
            home_url: String::new(),
            default_zoom: 1.0,
            tab_double_click_close: true,
            tab_show_close: true,
            tab_close_goes_left: true,
            tab_right_click_close: false,
            tab_reuse_blank: true,
            tab_force_current: false,
            tab_force_background: false,
            tab_pinned_menu_only: false,
            tab_small_inherit: true,
            tab_wheel_switch: false,
            tab_close_warn: false,
            tab_shape: default_tab_shape(),
            tab_min_width: default_tab_min_width(),
            tab_list_button: true,
            tab_scroll_buttons: true,
            tab_hover_activate: 0,
            tab_scroll_speed: default_tab_scroll_speed(),
            tab_audio_click: default_audio_click(),
            open_bookmark: default_open_foreground(),
            open_home: default_open_foreground(),
            open_search: default_open_foreground(),
            open_address: default_open_current(),
            tab_new_position: default_tab_position(),
            tab_close_to: default_close_to(),
            tab_last_close: default_last_close(),
            tab_middle_click: default_middle_tab(),
            tab_middle_strip: default_middle_strip(),
            custom_search: String::new(),
            newtab_show_search: true,
            newtab_show_sites: true,
            newtab_url: String::new(),
            newtab_tiles: 12,
            newtab_columns: 6,
            newtab_radius: 50,
            shortcuts: std::collections::BTreeMap::new(),
            search_engine: "baidu".into(),
            new_tab_page: true,
            language: "en".into(),
            memory_saver: true,
            keep_alive_hosts: String::new(),
            mark_suspended: true,
            tab_hover_preview: false,
            freeze_background: true,
            reset_extensions: false,
            live_thumbs: false,
            thumb_show_memory: true,
            max_live_tabs: 12,
            download_dir: None,
            groups_in_bookmarks: false,
            group_auto_pin: false,
            tab_search_place: default_search_place(),
            autoscroll: true,
            middle_click_paste: false,
            copy_on_select: true,
            fullscreen_hint: true,
            video_popup_hint: true,
            remember_popup_position: false,
            ignore_system_dpi: false,
            eye_comfort: false,
            eye_comfort_color: default_eye_color(),
            external_schemes: default_external_schemes(),
        }
    }
}

impl Settings {
    /// Keeps older profiles in step with changes to the default toolbar.
    fn migrate(mut self) -> Self {
        if !self.toolbar.iter().any(|id| id == "zoom") {
            for id in self.toolbar.iter_mut() {
                if id == "find" {
                    *id = "zoom".to_string();
                }
            }
        }
        if self.toolbar.is_empty() {
            self.toolbar = Settings::default().toolbar;
        }
        self
    }

    /// Every rule the settings have to satisfy before they are used or written.
    ///
    /// It runs on the way in - a patch from the ui, a file from disk - instead of at each
    /// field read, so there is one answer to "which values are allowed" rather than one
    /// per call site. Nothing here trusts its input: the page and the file are both
    /// outside this process's control.
    pub fn sanitize(&mut self) {
        self.schema_version = SCHEMA;

        self.proxy_mode = match self.proxy_mode.as_str() {
            "manual" => "manual".to_string(),
            "off" => "off".to_string(),
            _ => default_proxy_mode(),
        };
        self.proxy_server = self.proxy_server.trim().to_string();

        self.sidebar = match self.sidebar.as_str() {
            "left" => "left".to_string(),
            "right" => "right".to_string(),
            _ => default_sidebar(),
        };
        self.sidebar_width =
            finite(self.sidebar_width, default_sidebar_width()).clamp(180.0, 640.0);

        // The two spellings of "is the bar shown": `show_bookmarks` predates `bookmark_bar_mode`
        // and is still what the older files carry, so a file with only the old field is read
        // as "always" and a file with only the new one sets the old one for the callers that
        // still ask it. After this the two never disagree.
        self.bookmark_bar_mode = match self.bookmark_bar_mode.as_str() {
            "always" | "never" | "address" => self.bookmark_bar_mode.clone(),
            _ => default_bar_mode(),
        };
        if self.show_bookmarks && self.bookmark_bar_mode == "never" {
            self.bookmark_bar_mode = "always".to_string();
        }
        self.show_bookmarks = self.bookmark_bar_mode != "never";
        self.bookmark_column_width =
            finite(self.bookmark_column_width, default_bookmark_column()).clamp(120.0, 400.0);
        self.bookmark_button_width =
            finite(self.bookmark_button_width, default_bookmark_button()).clamp(40.0, 320.0);
        // The edge reveal is a distance inside the window's left edge; past a third of a
        // window it stops being "the edge" and starts catching ordinary clicks.
        self.bookmark_reveal_edge = self.bookmark_reveal_edge.min(200);
        self.bookmark_reveal_delay = self.bookmark_reveal_delay.clamp(50, 2000);
        self.bookmark_hide_delay = self.bookmark_hide_delay.clamp(100, 5000);
        // Only the panels this build knows, in their standard order, and never none.
        let known = default_sidebar_views();
        if self.sidebar_views.is_empty() || self.sidebar_views.iter().any(|id| !known.contains(id))
        {
            let asked = std::mem::take(&mut self.sidebar_views);
            self.sidebar_views = known
                .into_iter()
                .filter(|id| asked.is_empty() || asked.iter().any(|entry| entry == id))
                .collect();
        }

        let toolbar = [
            "downloads",
            "translate",
            "find",
            "zoom",
            "bookmark",
            "extensions",
            "home",
            "reopen",
            "mute",
            "shot",
        ];
        self.toolbar.retain(|id| toolbar.contains(&id.as_str()));
        if self.toolbar.is_empty() {
            self.toolbar = Settings::default().toolbar;
        }

        self.keep_alive_hosts = self.keep_alive_hosts.trim().to_string();
        // Hosts, not urls: a list that accumulates junk entries stops matching anything, and
        // the comparison at page load is exact.
        self.translate_hosts = self
            .translate_hosts
            .iter()
            .map(|host| host.trim().trim_start_matches("www.").to_ascii_lowercase())
            .filter(|host| !host.is_empty())
            .collect();
        self.translate_hosts.dedup();
        self.translate_hosts.truncate(200);
        self.drag_threshold = self.drag_threshold.clamp(8, 120);
        self.gesture_width = self.gesture_width.clamp(1, 10);
        self.gesture_threshold = self.gesture_threshold.clamp(10, 200);
        self.gesture_color = self.gesture_color.trim().to_string();
        self.gesture_blocked = self.gesture_blocked.trim().to_string();

        self.theme_mode = match self.theme_mode.as_str() {
            "light" => "light".to_string(),
            "system" => "system".to_string(),
            _ => "dark".to_string(),
        };
        // Only the three places the panel can actually open at; anything else is the
        // default rather than a value the layout code has to defend against.
        self.tab_search_place = match self.tab_search_place.as_str() {
            "left" => "left".to_string(),
            "center" => "center".to_string(),
            _ => default_search_place(),
        };
        self.font_family = self.font_family.trim().to_string();
        self.font_mono = self.font_mono.trim().to_string();
        // A tint is pasted into a stylesheet, so only a colour that parses is kept; a
        // half-typed one from a text field would break every page's `filter` at once.
        self.eye_comfort_color =
            normalize_hex(&self.eye_comfort_color).unwrap_or_else(default_eye_color);
        // The scheme list is split on the separators the row's own hint names, lowercased,
        // and stripped of anything that is not a scheme: a page hands over `a:b` and the
        // part before the colon is what decides who handles it.
        self.external_schemes = self
            .external_schemes
            .split([',', ';', ' ', '\n', '\r', '\t'])
            .map(|entry| entry.trim().trim_end_matches(':').to_ascii_lowercase())
            .filter(|entry| {
                !entry.is_empty()
                    && entry.len() <= 32
                    && entry
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            })
            .collect::<Vec<_>>()
            .join(",");
        self.font_size = self.font_size.min(32);
        self.font_min_size = self.font_min_size.min(24);
        self.home_url = self.home_url.trim().to_string();
        self.default_zoom = finite(self.default_zoom, 1.0).clamp(0.5, 2.0);

        // The tab strip: four fixed sets of answers, one shape, and the numbers the strip's
        // own layout uses.
        self.tab_shape = one_of(&self.tab_shape, &["rounded", "square"], default_tab_shape);
        self.tab_min_width = finite(self.tab_min_width, default_tab_min_width()).clamp(60.0, 260.0);
        self.tab_scroll_speed = self.tab_scroll_speed.clamp(1, 12);
        self.tab_hover_activate = if self.tab_hover_activate == 0 {
            0
        } else {
            self.tab_hover_activate.clamp(100, 2000)
        };
        self.tab_audio_click = one_of(
            &self.tab_audio_click,
            &["mute", "activate"],
            default_audio_click,
        );
        for (field, fallback) in [
            (
                &mut self.open_bookmark,
                default_open_foreground as fn() -> String,
            ),
            (&mut self.open_home, default_open_foreground),
            (&mut self.open_search, default_open_foreground),
        ] {
            *field = one_of(field, &["foreground", "background", "current"], fallback);
        }
        self.open_address = one_of(
            &self.open_address,
            &["current", "foreground", "background"],
            default_open_current,
        );
        self.tab_new_position = one_of(
            &self.tab_new_position,
            &["after_current", "after_group", "at_end"],
            default_tab_position,
        );
        self.tab_close_to = one_of(
            &self.tab_close_to,
            &["auto", "left", "right", "last_used"],
            default_close_to,
        );
        self.tab_last_close = one_of(
            &self.tab_last_close,
            &["new_tab", "close_window", "nothing"],
            default_last_close,
        );
        self.tab_middle_click = one_of(
            &self.tab_middle_click,
            &["close", "reload", "nothing"],
            default_middle_tab,
        );
        self.tab_middle_strip = one_of(
            &self.tab_middle_strip,
            &["new_tab", "close_others", "nothing"],
            default_middle_strip,
        );

        self.custom_search = self.custom_search.trim().to_string();
        self.newtab_url = self.newtab_url.trim().to_string();
        self.newtab_tiles = self.newtab_tiles.clamp(1, 40);
        self.newtab_columns = self.newtab_columns.clamp(1, 12);
        self.newtab_radius = self.newtab_radius.min(50);

        let combos = std::mem::take(&mut self.shortcuts);
        self.shortcuts = combos
            .into_iter()
            .filter(|(_, combo)| !combo.trim().is_empty())
            .map(|(action, combo)| (action, normalize_combo(&combo)))
            .collect();

        self.search_engine = match self.search_engine.as_str() {
            "bing" => "bing".to_string(),
            "google" => "google".to_string(),
            "ddg" => "ddg".to_string(),
            _ => "baidu".to_string(),
        };
        self.language = if self.language == "zh" {
            "zh".to_string()
        } else {
            "en".to_string()
        };
        self.translate_to = match self.translate_to.trim() {
            "" => "zh".to_string(),
            value => value.to_string(),
        };
        if self.start_url.trim().is_empty() {
            self.start_url = Settings::default().start_url;
        }
        self.max_live_tabs = self.max_live_tabs.clamp(1, 64);
        self.chrome_height = finite(self.chrome_height, 0.0).clamp(0.0, 400.0);
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        crate::paths::write_atomic(path, json.as_bytes())
    }

    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&raw) {
            Ok(value) => {
                if value.schema_version > SCHEMA {
                    crate::logging::warn("settings.json is newer than this build; using defaults");
                    return Self::default();
                }
                let mut value = value.migrate();
                value.sanitize();
                // Measured, never stored: an old file may still carry the height the strip
                // used to be, and trusting it would put the pages 16 px too low until the
                // strip measured itself again.
                value.chrome_height = 0.0;
                value
            }
            Err(err) => {
                crate::logging::warn(&format!(
                    "settings.json unreadable ({err}); keeping a backup"
                ));
                let _ = std::fs::rename(path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }
}

/// A number that came from outside; NaN and infinity are replaced by a sane one.
fn finite(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// A colour written the way a stylesheet wants it, or nothing.
///
/// Accepts `#RGB` and `#RRGGBB` with or without the hash, and returns the six-digit form in
/// upper case. Anything else is not a colour: it is a typo, and letting it through would
/// put an unparsable value in front of every page.
fn normalize_hex(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('#');
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    match digits.len() {
        3 => {
            let mut out = String::from("#");
            for ch in digits.chars() {
                out.push(ch);
                out.push(ch);
            }
            Some(out.to_ascii_uppercase())
        }
        6 => Some(format!("#{}", digits.to_ascii_uppercase())),
        _ => None,
    }
}

/// Stores key combos in one spelling: "ctrl+shift+t".
pub(crate) fn normalize_combo(raw: &str) -> String {
    let mut parts: Vec<String> = raw
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .map(|part| match part.as_str() {
            "control" => "ctrl".to_string(),
            "esc" => "escape".to_string(),
            "del" => "delete".to_string(),
            other => other.to_string(),
        })
        .collect();
    parts.sort_by_key(|part| match part.as_str() {
        "ctrl" => 0,
        "alt" => 1,
        "shift" => 2,
        "meta" => 3,
        _ => 4,
    });
    parts.join("+")
}
#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(dir);
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("settings.json");
        std::fs::write(&file, body).unwrap();
        file
    }

    #[test]
    fn the_tab_rows_fall_back_when_their_answer_is_not_one_of_them() {
        // Every one of these is a select, so a value outside the list can only come from a
        // hand-edited file or an older build. The default is the safe answer: the code that
        // reads them matches exhaustively and would otherwise carry a branch nothing can
        // reach.
        let file = write(
            "mix-settings-tabs",
            r#"{"schema_version":1,"tab_shape":"hexagonal","tab_close_to":"somewhere",
                "tab_last_close":"explode","tab_middle_click":"sing","tab_audio_click":"dance",
                "open_bookmark":"orbit","tab_min_width":9000,"tab_scroll_speed":400,
                "tab_hover_activate":17}"#,
        );
        let settings = Settings::load(&file);
        assert_eq!(settings.tab_shape, "rounded");
        assert_eq!(settings.tab_close_to, "auto");
        assert_eq!(settings.tab_last_close, "new_tab");
        assert_eq!(settings.tab_middle_click, "close");
        assert_eq!(settings.tab_audio_click, "mute");
        assert_eq!(settings.open_bookmark, "foreground");
        assert_eq!(settings.tab_min_width, 260.0);
        assert_eq!(settings.tab_scroll_speed, 12);
        // Zero is the off position and is not a duration; anything else is a wait.
        assert_eq!(settings.tab_hover_activate, 100);
    }

    #[test]
    fn a_duration_that_was_off_stays_off() {
        let mut settings = Settings {
            tab_hover_activate: 0,
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(
            settings.tab_hover_activate, 0,
            "a hover delay that is off must not become 100 ms"
        );
    }

    #[test]
    fn an_old_toolbar_gains_the_zoom_button() {
        let file = write(
            "mix-settings-migrate",
            r#"{"schema_version":1,"toolbar":["downloads","translate","find","bookmark","extensions"]}"#,
        );
        let settings = Settings::load(&file);
        assert!(settings.toolbar.iter().any(|id| id == "zoom"));
        assert!(!settings.toolbar.iter().any(|id| id == "find"));
    }

    #[test]
    fn a_missing_toolbar_falls_back_to_the_default() {
        let file = write("mix-settings-empty", r#"{"schema_version":1}"#);
        let settings = Settings::load(&file);
        assert_eq!(settings.toolbar, Settings::default().toolbar);
    }

    #[test]
    fn an_explicit_choice_is_kept() {
        let file = write(
            "mix-settings-keep",
            r#"{"schema_version":1,"toolbar":["zoom","bookmark"]}"#,
        );
        let settings = Settings::load(&file);
        assert_eq!(
            settings.toolbar,
            vec!["zoom".to_string(), "bookmark".to_string()]
        );
    }

    #[test]
    fn a_hand_edited_file_is_clamped_on_the_way_in() {
        let file = write(
            "mix-settings-sanitize",
            r#"{"schema_version":1,"theme_mode":"solarized","max_live_tabs":900,
                "sidebar_width":-4.0,"toolbar":["zoom","nonsense"],"language":"fr"}"#,
        );
        let settings = Settings::load(&file);
        assert_eq!(settings.theme_mode, "dark");
        assert_eq!(settings.max_live_tabs, 64);
        assert_eq!(settings.sidebar_width, 180.0);
        assert_eq!(settings.toolbar, vec!["zoom".to_string()]);
        assert_eq!(settings.language, "en");
    }

    #[test]
    fn a_nonsense_number_does_not_become_nan() {
        let mut settings = Settings {
            sidebar_width: f64::NAN,
            default_zoom: f64::INFINITY,
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.sidebar_width, default_sidebar_width());
        assert_eq!(settings.default_zoom, 1.0);
    }

    #[test]
    fn the_panels_the_build_knows_survive_in_their_order() {
        let mut settings = Settings {
            sidebar_views: vec![
                "history".to_string(),
                "gone".to_string(),
                "bookmarks".to_string(),
            ],
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(
            settings.sidebar_views,
            vec!["bookmarks".to_string(), "history".to_string()]
        );
    }

    #[test]
    fn key_combos_are_stored_in_one_spelling() {
        assert_eq!(normalize_combo("Shift + Control + T"), "ctrl+shift+t");
        assert_eq!(normalize_combo("ESC"), "escape");
    }
}
