import { setLanguage, t } from "../i18n.js";
import { row, toggle, switchEl } from "./settings/ui.js";
import { addNavItem, groupSections } from "./settings/shell.js";
import { buildGestures } from "./settings/sections/gestures.js";
import { buildDrag } from "./settings/sections/drag.js";
import { buildShortcuts } from "./settings/sections/shortcuts.js";
import { TOOLBAR_CHOICES, DEFAULT_TOOLBAR } from "./settings/tables.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

async function boot() {
  let settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Settings");

  const pages = {
    history: "http://tauri.localhost/pages/history.html",
    extensions: "http://tauri.localhost/pages/extensions.html",
  };
  addNavItem(nav, "⟲", "History", false, pages.history);
  addNavItem(nav, "☆", "Bookmarks", false, null);
  addNavItem(nav, "⧉", "Extensions", false, pages.extensions);
  addNavItem(nav, "⚙", "Settings", true, null);

  const head = document.createElement("h2");
  head.textContent = t("Settings");
  main.append(head);

  // Appended right here, not at the end of boot: the page is folded into
  // sections by the h2 that precedes each card, so leaving it to the end put
  // session, bookmarks bar and search engine under "Default browser".
  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  card.append(
    row(
      "Restore session",
      "Restore the last session on start",
      toggle(settings.restore_session, async (value) => {
        await invoke("settings_set", { patch: { restore_session: value } }).catch(() => {});
      }),
    ),
  );

  card.append(
    row(
      "Bookmarks bar",
      "Show the bookmarks bar",
      toggle(settings.show_bookmarks, async (value) => {
        await invoke("settings_set", { patch: { show_bookmarks: value } }).catch(() => {});
      }),
    ),
  );

  const language = document.createElement("select");
  language.className = "button";
  for (const [value, label] of [["en", "English"], ["zh", "Simplified Chinese"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.language || "en") === value) option.selected = true;
    language.append(option);
  }
  language.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { language: language.value } }).catch(() => {});
    setLanguage(language.value);
    location.reload();
  });

  const zoomSelect = document.createElement("select");
  zoomSelect.className = "button";
  for (const level of [0.8, 0.9, 1.0, 1.1, 1.25, 1.5]) {
    const option = document.createElement("option");
    option.value = String(level);
    option.textContent = Math.round(level * 100) + "%";
    if (Math.abs((settings.default_zoom ?? 1) - level) < 0.01) option.selected = true;
    zoomSelect.append(option);
  }
  zoomSelect.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { default_zoom: Number(zoomSelect.value) },
    }).catch(() => {});
  });
  const zoomRow = row("Page zoom", "Applied to newly opened pages", zoomSelect);

  const fontInput = document.createElement("input");
  fontInput.className = "hex";
  fontInput.style.width = "240px";
  fontInput.placeholder = "Microsoft YaHei";
  fontInput.value = settings.font_family || "";
  fontInput.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_family: fontInput.value } }).catch(() => {});
  });

  // Size presets, like the appearance pages of the browsers this imitates:
  // 0 keeps whatever the site asked for.
  const SIZE_PRESETS = [
    [0, "Site default"],
    [12, "Small"],
    [14, "Medium"],
    [16, "Large"],
    [18, "Extra large"],
  ];
  const sizeSelect = document.createElement("select");
  sizeSelect.className = "button";
  for (const [size, label] of SIZE_PRESETS) {
    const option = document.createElement("option");
    option.value = String(size);
    option.textContent = size === 0 ? t(label) : `${t(label)} (${size}px)`;
    if ((settings.font_size ?? 0) === size) option.selected = true;
    sizeSelect.append(option);
  }
  sizeSelect.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_size: Number(sizeSelect.value) } }).catch(() => {});
  });

  const MIN_SIZES = [0, 10, 11, 12, 13, 14, 16, 18];
  const minSelect = document.createElement("select");
  minSelect.className = "button";
  for (const size of MIN_SIZES) {
    const option = document.createElement("option");
    option.value = String(size);
    option.textContent = size === 0 ? t("Off") : size + " px";
    if ((settings.font_min_size ?? 0) === size) option.selected = true;
    minSelect.append(option);
  }
  minSelect.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { font_min_size: Number(minSelect.value) },
    }).catch(() => {});
  });

  const monoInput = document.createElement("input");
  monoInput.className = "hex";
  monoInput.style.width = "240px";
  monoInput.placeholder = "JetBrains Mono";
  monoInput.value = settings.font_mono || "";
  monoInput.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_mono: monoInput.value } }).catch(() => {});
  });

  // Live preview: the same sample line drawn twice, once in each family.
  const preview = document.createElement("div");
  preview.style.padding = "10px 0 2px";
  const previewText = document.createElement("div");
  previewText.textContent = "16: The quick brown fox jumps over the lazy dog";
  previewText.style.fontSize = "16px";
  const previewMono = document.createElement("div");
  previewMono.textContent = "13: const mix = await open(url);";
  previewMono.style.fontSize = "13px";
  preview.append(previewText, previewMono);
  const paintPreview = () => {
    previewText.style.fontFamily = fontInput.value.trim() || "";
    previewMono.style.fontFamily =
      monoInput.value.trim() || "ui-monospace, Courier New, monospace";
  };
  fontInput.addEventListener("input", paintPreview);
  monoInput.addEventListener("input", paintPreview);
  paintPreview();

  const engine = document.createElement("select");
  engine.className = "button";
  for (const [value, label] of [
    ["baidu", "Baidu"],
    ["bing", "Bing"],
    ["google", "Google"],
    ["ddg", "DuckDuckGo"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    if ((settings.search_engine || "baidu") === value) option.selected = true;
    engine.append(option);
  }
  engine.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { search_engine: engine.value } }).catch(() => {});
  });
  card.append(row("Search engine", "Used when the address bar is not a url", engine));

  const customSearch = document.createElement("input");
  customSearch.className = "hex";
  customSearch.style.width = "320px";
  customSearch.placeholder = "https://example.com/search?q=%s";
  customSearch.value = settings.custom_search || "";
  customSearch.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { custom_search: customSearch.value } }).catch(() => {});
  });
  card.append(
    row(
      "Custom search url",
      "Overrides the engine above; %s is the query",
      customSearch,
    ),
  );

  const cssButton = document.createElement("button");
  cssButton.className = "button";
  cssButton.textContent = t("Edit custom CSS");
  const cssNote = document.createElement("span");
  cssNote.className = "card-note";
  cssButton.addEventListener("click", async () => {
    const path = await invoke("custom_css_open").catch((error) => "error: " + String(error));
    cssNote.textContent = String(path);
  });
  const cssRow = document.createElement("div");
  cssRow.className = "card-actions";
  cssRow.append(cssButton, cssNote);
  card.append(
    row("Custom CSS", "UserData/custom.css is applied to every page; restart to reload"),
    cssRow,
  );

  card.append(row("Language", "Choose the language of the interface", language));
  main.append(card);

  const dataHead = document.createElement("h2");
  dataHead.textContent = t("Clear browsing data");
  dataHead.style.margin = "18px 0 10px";
  main.append(dataHead);

  const dataCard = document.createElement("div");
  dataCard.className = "card";
  dataCard.style.maxWidth = "720px";

  const KINDS = {
    history: "History",
    bookmarks: "Bookmarks",
    favicons: "Site icons",
    zoom: "Zoom levels",
    session: "Session",
  };
  const picked = new Set(["history"]);

  for (const [kind, label] of Object.entries(KINDS)) {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "pick";
    box.checked = picked.has(kind);
    box.addEventListener("change", () => {
      if (box.checked) picked.add(kind);
      else picked.delete(kind);
    });
    const line = document.createElement("div");
    line.className = "entry";
    const text = document.createElement("span");
    text.className = "title";
    text.textContent = t(label);
    line.append(box, text);
    dataCard.append(line);
  }

  const run = document.createElement("button");
  run.className = "button";
  run.textContent = t("Clear selected data");
  run.addEventListener("click", async () => {
    // Irreversible: history, bookmarks, session and zoom have no undo.
    if (!window.confirm(t("Clear the selected data? This cannot be undone."))) return;
    const done = await invoke("clear_data", { kinds: [...picked] }).catch(() => []);
    run.textContent = t("Cleared") + ": " + (done || []).join(", ");
    setTimeout(() => {
      run.textContent = t("Clear selected data");
    }, 2500);
  });
  const runRow = document.createElement("div");
  runRow.className = "card-actions";
  runRow.append(run);
  dataCard.append(runRow);
  main.append(dataCard);

  const startHead = document.createElement("h2");
  startHead.textContent = t("Startup");
  startHead.style.margin = "18px 0 10px";
  main.append(startHead);

  const startCard = document.createElement("div");
  startCard.className = "card";
  startCard.style.maxWidth = "720px";

  const startUrl = document.createElement("input");
  startUrl.className = "hex";
  startUrl.style.width = "320px";
  startUrl.value = settings.start_url || "";
  startUrl.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { start_url: startUrl.value } }).catch(() => {});
  });
  const homeUrl = document.createElement("input");
  homeUrl.className = "hex";
  homeUrl.style.width = "320px";
  homeUrl.placeholder = settings.start_url || "https://example.com";
  homeUrl.value = settings.home_url || "";
  homeUrl.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { home_url: homeUrl.value } }).catch(() => {});
  });
  startCard.append(
    row("Start page", "Opened at launch", startUrl),
    row(t("Home address"), t("Opened by the home button; empty uses the start page"), homeUrl),
  );
  main.append(startCard);

  const themeHead = document.createElement("h2");
  themeHead.textContent = t("Appearance");
  themeHead.style.margin = "18px 0 10px";
  main.append(themeHead);

  const themeCard = document.createElement("div");
  themeCard.className = "card";
  themeCard.style.maxWidth = "720px";
  const themeNote = document.createElement("span");
  themeNote.className = "card-note";

  const themeExport = document.createElement("button");
  themeExport.className = "button";
  themeExport.textContent = t("Export theme");
  themeExport.addEventListener("click", async () => {
    const path = await invoke("theme_export").catch((error) => "error: " + String(error));
    themeNote.textContent = String(path);
  });

  const themeImport = document.createElement("button");
  themeImport.className = "button";
  themeImport.textContent = t("Import theme");
  themeImport.addEventListener("click", async () => {
    const result = await invoke("theme_import").catch((error) => "error: " + String(error));
    themeNote.textContent = t("Theme loaded") + ": " + String(result);
  });

  const themeRow = document.createElement("div");
  themeRow.className = "card-actions";
  themeRow.append(themeExport, themeImport, themeNote);
  const themeText = document.createElement("div");
  themeText.className = "card-note";
  themeText.textContent = t("theme.json can be shared with the same folder layout");
  const modeSelect = document.createElement("select");
  modeSelect.className = "button";
  for (const [value, label] of [["dark", "Dark"], ["light", "Light"], ["system", "Follow system"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.theme_mode || "dark") === value) option.selected = true;
    modeSelect.append(option);
  }
  modeSelect.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { theme_mode: modeSelect.value } }).catch(() => {});
    location.reload();
  });

  themeCard.append(
    themeText,
    row("Mode", "Dark or light palette for the interface", modeSelect),
    themeRow,
    zoomRow,
    row(t("Standard font"), t("Empty keeps the site font, applies to new pages"), fontInput),
    row(t("Monospace font"), t("Used for code blocks and text areas"), monoInput),
    row(t("Font size"), t("Text size for pages that do not set one"), sizeSelect),
    row(t("Minimum font size"), t("Stops pages from rendering text smaller than this"), minSelect),
    preview,
  );
  main.append(themeCard);

  const perfHead = document.createElement("h2");
  perfHead.textContent = t("Performance");
  perfHead.style.margin = "18px 0 10px";
  main.append(perfHead);

  const perfCard = document.createElement("div");
  perfCard.className = "card";
  perfCard.style.maxWidth = "720px";

  const perfSwitch = switchEl(settings.memory_saver !== false, async (next) => {
    await invoke("settings_set", { patch: { memory_saver: next } }).catch(() => {});
    liveInput.disabled = !next;
  });

  const liveInput = document.createElement("input");
  liveInput.type = "range";
  liveInput.min = "2";
  liveInput.max = "40";
  liveInput.value = String(settings.max_live_tabs ?? 12);
  liveInput.style.width = "220px";
  liveInput.disabled = settings.memory_saver === false;
  const liveValue = document.createElement("span");
  liveValue.className = "card-note";
  liveValue.textContent = liveInput.value;
  liveInput.addEventListener("input", () => {
    liveValue.textContent = liveInput.value;
  });
  liveInput.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { max_live_tabs: Number(liveInput.value) },
    }).catch(() => {});
  });
  const liveHolder = document.createElement("div");
  liveHolder.style.display = "flex";
  liveHolder.style.gap = "10px";
  liveHolder.style.alignItems = "center";
  liveHolder.append(liveInput, liveValue);

  const keepAlive = document.createElement("input");
  keepAlive.className = "hex";
  keepAlive.style.width = "320px";
  keepAlive.placeholder = "bilibili.com, mail.google.com";
  keepAlive.value = settings.keep_alive_hosts || "";
  keepAlive.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { keep_alive_hosts: keepAlive.value } }).catch(() => {});
  });

  const markSwitch = switchEl(settings.mark_suspended !== false, async (next) => {
    await invoke("settings_set", { patch: { mark_suspended: next } }).catch(() => {});
  });

  perfCard.append(
    row(
      t("Memory saver"),
      t("Tabs beyond the limit are suspended and reload when selected"),
      perfSwitch,
    ),
    row(t("Keep this many tabs live"), "", liveHolder),
    row(t("Never suspend these sites"), t("Comma separated hosts"), keepAlive),
    row(t("Mark suspended tabs"), t("Dashed border and grey icon"), markSwitch),
  );
  main.append(perfCard);

  const tabsHead = document.createElement("h2");
  tabsHead.textContent = t("Tab settings");
  tabsHead.style.margin = "18px 0 10px";
  main.append(tabsHead);

  const tabsCard = document.createElement("div");
  tabsCard.className = "card";
  tabsCard.style.maxWidth = "720px";

  const tabToggle = (label, note, value, key) => {
    const control = switchEl(value !== false, async (next) => {
      await invoke("settings_set", { patch: { [key]: next } }).catch(() => {});
    });
    return row(t(label), t(note), control);
  };

  tabsCard.append(
    tabToggle(
      "Double click closes a tab",
      "Off means a double click only selects it",
      settings.tab_double_click_close,
      "tab_double_click_close",
    ),
    tabToggle(
      "Show the close button",
      "Tabs can still be closed with Ctrl+W or the middle click",
      settings.tab_show_close,
      "tab_show_close",
    ),
    tabToggle(
      "Activate the tab on the left after closing",
      "Off selects the tab on the right",
      settings.tab_close_goes_left,
      "tab_close_goes_left",
    ),
  );

  // The gesture and drag sections live in their own modules: they are mostly
  // tables, and this file only has to place them.
  main.append(tabsCard);
  main.append(...buildDrag({ settings, invoke }));
  main.append(...buildGestures({ settings, invoke }));

  const newtabHead = document.createElement("h2");
  newtabHead.textContent = t("New tab page");
  newtabHead.style.margin = "18px 0 10px";
  main.append(newtabHead);

  const newtabCard = document.createElement("div");
  newtabCard.className = "card";
  newtabCard.style.maxWidth = "720px";

  const toggleRow = (label, value, key) => {
    const control = switchEl(value !== false, async (next) => {
      await invoke("settings_set", { patch: { [key]: next } }).catch(() => {});
    });
    return row(t(label), "", control);
  };

  newtabCard.append(
    toggleRow("Show search box", settings.newtab_show_search, "newtab_show_search"),
    toggleRow("Show frequent sites", settings.newtab_show_sites, "newtab_show_sites"),
  );

  const urlInput2 = document.createElement("input");
  urlInput2.className = "hex";
  urlInput2.style.width = "320px";
  urlInput2.placeholder = "https://example.com";
  urlInput2.value = settings.newtab_url || "";
  urlInput2.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { newtab_url: urlInput2.value } }).catch(() => {});
  });
  newtabCard.append(row(t("Custom address"), t("Opens instead of this page"), urlInput2));

  const slider = (label, value, key, min, max) => {
    const input = document.createElement("input");
    input.type = "range";
    input.min = String(min);
    input.max = String(max);
    input.value = String(value);
    input.style.width = "220px";
    const shown = document.createElement("span");
    shown.className = "card-note";
    shown.textContent = String(value);
    input.addEventListener("input", () => {
      shown.textContent = input.value;
    });
    input.addEventListener("change", async () => {
      await invoke("settings_set", { patch: { [key]: Number(input.value) } }).catch(() => {});
    });
    const holder = document.createElement("div");
    holder.style.display = "flex";
    holder.style.alignItems = "center";
    holder.style.gap = "10px";
    holder.append(input, shown);
    return row(t(label), "", holder);
  };

  newtabCard.append(
    slider("Max tiles", settings.newtab_tiles ?? 12, "newtab_tiles", 1, 40),
    slider("Max columns", settings.newtab_columns ?? 6, "newtab_columns", 1, 12),
    slider("Tile radius", settings.newtab_radius ?? 50, "newtab_radius", 0, 50),
  );
  main.append(newtabCard);

  // Shortcut fields and the duplicate warning live in their own module.
  main.append(...buildShortcuts({ settings, invoke }));
  checkConflicts();

  const backupHead = document.createElement("h2");
  backupHead.textContent = t("Backup");
  backupHead.style.margin = "18px 0 10px";
  main.append(backupHead);

  const backupCard = document.createElement("div");
  backupCard.className = "card";
  backupCard.style.maxWidth = "720px";

  const backupNote = document.createElement("span");
  backupNote.className = "card-note";

  const exportButton = document.createElement("button");
  exportButton.className = "button";
  exportButton.textContent = t("Export profile");
  exportButton.addEventListener("click", async () => {
    const path = await invoke("backup_export").catch((error) => "error: " + String(error));
    backupNote.textContent = String(path);
  });

  const importButton = document.createElement("button");
  importButton.className = "button";
  importButton.textContent = t("Import profile");
  importButton.addEventListener("click", async () => {
    const result = await invoke("backup_import").catch((error) => "error: " + String(error));
    backupNote.textContent = t("Restored") + ": " + String(result) + " — " + t("restart to apply");
  });

  const backupRow = document.createElement("div");
  backupRow.className = "card-actions";
  backupRow.append(exportButton, importButton, backupNote);
  const backupText = document.createElement("div");
  backupText.className = "card-note";
  backupText.textContent = t(
    "Settings, history, bookmarks, theme and icons without the WebView2 profile",
  );
  backupCard.append(backupText, backupRow);
  main.append(backupCard);

  const browserHead = document.createElement("h2");
  browserHead.textContent = t("Default browser");
  browserHead.style.margin = "18px 0 10px";
  main.append(browserHead);

  const browserCard = document.createElement("div");
  browserCard.className = "card";
  browserCard.style.maxWidth = "720px";

  const register = document.createElement("button");
  register.className = "button";
  register.textContent = t("Register as a browser");
  const status = document.createElement("span");
  status.className = "card-note";
  register.addEventListener("click", async () => {
    const result = await invoke("register_browser").catch((error) => "error: " + String(error));
    status.textContent = String(result);
  });
  const row2 = document.createElement("div");
  row2.className = "card-actions";
  row2.append(register, status);
  browserCard.append(
    row(t("Default browser"), t("Adds Mix to the Windows default apps list")),
    row2,
  );
  main.append(browserCard);

  const toolbarHead = document.createElement("div");
  toolbarHead.style.marginTop = "14px";
  main.append(toolbarHead);

  const toolbarCard = document.createElement("div");
  toolbarCard.className = "card";
  toolbarCard.style.maxWidth = "720px";

  // The strip draws these from settings.toolbar, and the order in that array is
  // the order they appear in, so a position is just an index.
  const toolbarIds = () =>
    Array.isArray(settings.toolbar) && settings.toolbar.length
      ? [...settings.toolbar]
      : [...DEFAULT_TOOLBAR];
  const saveToolbar = async (next) => {
    settings.toolbar = next;
    await invoke("settings_set", { patch: { toolbar: next } }).catch(() => {});
  };

  for (const [id, label] of TOOLBAR_CHOICES) {
    toolbarCard.append(
      row(
        t(label),
        "",
        toggle(toolbarIds().includes(id), async (on) => {
          const list = toolbarIds().filter((item) => item !== id);
          if (on) list.push(id);
          await saveToolbar(list);
        }),
      ),
    );
  }

  const bookmarkPlace = document.createElement("select");
  bookmarkPlace.className = "button";
  for (const [value, label] of [["left", "Left"], ["right", "Right"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((value === "left") === (toolbarIds()[0] === "bookmark")) option.selected = true;
    bookmarkPlace.append(option);
  }
  bookmarkPlace.addEventListener("change", async () => {
    const list = toolbarIds().filter((item) => item !== "bookmark");
    if (bookmarkPlace.value === "left") list.unshift("bookmark");
    else list.push("bookmark");
    await saveToolbar(list);
  });
  toolbarCard.append(row(t("Bookmark button position"), "", bookmarkPlace));

  const toolbarReset = document.createElement("button");
  toolbarReset.className = "button";
  toolbarReset.textContent = t("Reset to defaults");
  toolbarReset.addEventListener("click", () => saveToolbar([...DEFAULT_TOOLBAR]));
  const resetRow = document.createElement("div");
  resetRow.className = "card-actions";
  resetRow.append(toolbarReset);
  toolbarCard.append(resetRow);

  themeCard.append(toolbarHead, ...toolbarCard.childNodes);
}

boot().then(() => {
  groupSections(nav, main);
});