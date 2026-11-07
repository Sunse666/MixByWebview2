// Every list the settings page offers, in one place: the page itself is layout,
// these are the choices.

export const SIZE_PRESETS = [
  [0, "Site default"],
  [12, "Small"],
  [14, "Medium"],
  [16, "Large"],
  [18, "Extra large"],
];

export const MIN_SIZES = [0, 10, 11, 12, 13, 14, 16, 18];
export const ZOOM_LEVELS = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5];
export const LANGS = [
  ["en", "English"],
  ["zh", "Simplified Chinese"],
];
export const ENGINES = [
  ["baidu", "Baidu"],
  ["bing", "Bing"],
  ["google", "Google"],
  ["ddg", "DuckDuckGo"],
];

export const CLEAR_KINDS = {
  history: "History",
  bookmarks: "Bookmarks",
  favicons: "Site icons",
  zoom: "Zoom levels",
  session: "Session",
};

export const GESTURE_ACTIONS = {
  back: "Back",
  forward: "Forward",
  reload: "Reload",
  close_tab: "Close tab",
  new_tab: "New tab",
  new_incognito: "New incognito tab",
  reopen_tab: "Reopen closed tab",
  next_tab: "Next tab",
  prev_tab: "Previous tab",
  new_window: "New window",
  bookmark: "Bookmark this page",
  focus_url: "Focus the address bar",
  downloads: "Downloads",
  fullscreen: "Fullscreen",
  devtools: "Dev tools",
  zoom_in: "Zoom in",
  scroll_top: "Scroll to top",
  scroll_bottom: "Scroll to bottom",
  stop: "Stop loading",
  duplicate_tab: "Duplicate this tab",
  close_other_tabs: "Close the other tabs",
  toggle_pin: "Pin or unpin this tab",
  copy_url: "Copy the page url",
  open_home: "Open the home page",
  open_history: "Open history",
  open_bookmarks: "Open bookmarks",
  open_downloads: "Open downloads",
  open_extensions: "Open extensions",
  open_settings: "Open settings",
  translate: "Translate this page",
  zoom_out: "Zoom out",
  zoom_reset: "Reset the zoom",
  minimize: "Minimise",
  maximize: "Maximise or restore",
  close_window: "Close the window",
  none: "Nothing",
};

export const PATTERNS = [
  ["U", "\u2191"],
  ["D", "\u2193"],
  ["L", "\u2190"],
  ["R", "\u2192"],
  ["UD", "\u2191\u2193"],
  ["DU", "\u2193\u2191"],
  ["LR", "\u2190\u2192"],
  ["RL", "\u2192\u2190"],
  ["UR", "\u2191\u2192"],
  ["DL", "\u2193\u2190"],
  ["UL", "\u2191\u2190"],
  ["DR", "\u2193\u2192"],
];

export const DEFAULTS_GESTURE = {
  U: "scroll_top",
  D: "scroll_bottom",
  L: "back",
  R: "forward",
  UD: "reload",
  DU: "reopen_tab",
  LR: "next_tab",
  RL: "prev_tab",
  UR: "new_tab",
  DL: "close_tab",
  UL: "bookmark",
  DR: "downloads",
};

export const DRAG_ACTIONS = {
  none: "Nothing",
  open: "Open in a new tab",
  open_background: "Open in a background tab",
  copy: "Copy to the clipboard",
  copy_link_text: "Copy the link text",
  copy_link_both: "Copy the link text and url",
  search: "Search the text",
  search_in_site: "Search this site for the text",
  copy_sel_and_url: "Copy the text and the page url",
  find_next: "Find the text (next)",
  find_prev: "Find the text (previous)",
  save: "Save the target",
};

export const DRAG_DEFAULTS = { L: "open", R: "search", U: "copy", D: "save" };

export const SHORTCUT_ACTIONS = {
  new_tab: "New tab",
  close_tab: "Close tab",
  reopen_tab: "Reopen tab",
  next_tab: "Next tab",
  prev_tab: "Previous tab",
  focus_url: "Focus address bar",
  reload: "Reload",
  fullscreen: "Fullscreen",
  zoom_in: "Zoom in",
  zoom_out: "Zoom out",
  zoom_reset: "Reset zoom",
  downloads: "Downloads",
  bookmark: "Bookmark this page",
  devtools: "Dev tools",
};

export const SHORTCUT_DEFAULTS = {
  new_tab: "ctrl+t",
  close_tab: "ctrl+w",
  reopen_tab: "ctrl+shift+t",
  next_tab: "ctrl+tab",
  prev_tab: "ctrl+shift+tab",
  focus_url: "ctrl+l",
  reload: "ctrl+r",
  fullscreen: "f11",
  zoom_in: "ctrl+=",
  zoom_out: "ctrl+-",
  zoom_reset: "ctrl+0",
  downloads: "ctrl+j",
  bookmark: "ctrl+d",
  devtools: "f12",
};

// The strip draws these from settings.toolbar, and the order in that array is
// the order they appear in, so a position is just an index.
export const TOOLBAR_CHOICES = [
  ["home", "Home"],
  ["reopen", "Reopen closed tab"],
  ["mute", "Mute tab"],
  ["shot", "Take a screenshot"],
  ["translate", "Translate this page"],
  ["zoom", "Zoom"],
  ["bookmark", "Bookmark this page"],
  ["find", "Find in page"],
  ["extensions", "Extensions"],
  ["downloads", "Downloads"],
];

export const DEFAULT_TOOLBAR = ["translate", "zoom", "bookmark", "extensions", "downloads"];
