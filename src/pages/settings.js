import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

function navItem(glyph, label, active, page) {
  const item = document.createElement("div");
  item.className = "nav-item" + (active ? " active" : "");
  const icon = document.createElement("span");
  icon.textContent = glyph;
  const text = document.createElement("span");
  text.textContent = t(label);
  item.append(icon, text);
  if (page) {
    item.addEventListener("click", () => {
      invoke("tab_navigate", { url: page }).catch(() => invoke("tab_new", { url: page }).catch(() => {}));
    });
  }
  nav.append(item);
}

function row(title, detail, control) {
  const line = document.createElement("div");
  line.className = "entry";
  const text = document.createElement("span");
  text.className = "title";
  text.textContent = t(title);
  if (detail) text.title = detail;
  line.append(text);
  if (control) line.append(control);
  return line;
}

function themePreview() {
  const holder = document.createElement("div");
  holder.className = "card-actions";
  const edit = document.createElement("button");
  edit.className = "button";
  edit.textContent = t("Edit");
  edit.addEventListener("click", () => openThemeEditor(holder));
  holder.append(edit);
  return holder;
}

async function openThemeEditor(holder) {
  const theme = await invoke("theme_get").catch(() => null);
  if (!theme) return;

  holder.replaceChildren();

  const draft = {
    colors: { ...theme.colors },
    radius: theme.radius,
    tab_radius: theme.tab_radius,
    font_size: theme.font_size,
    font_family: theme.font_family,
  };

  const fields = [
    ["Background", "bg"],
    ["Chrome", "chrome"],
    ["Accent", "accent"],
    ["Text", "text"],
    ["Border", "border"],
    ["Tab", "tab"],
    ["URL field", "url_bg"],
  ];

  for (const [label, key] of fields) {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "hex";
    input.value = draft.colors[key] || "";
    input.placeholder = "#RRGGBB";
    input.addEventListener("input", () => {
      draft.colors[key] = input.value.trim();
    });
    holder.append(row(label, "", input));
  }

  for (const [label, key, min, max] of [
    ["Corner radius", "radius", 0, 20],
    ["Tab radius", "tab_radius", 0, 20],
    ["Font size", "font_size", 11, 20],
  ]) {
    const input = document.createElement("input");
    input.type = "number";
    input.className = "num";
    input.min = String(min);
    input.max = String(max);
    input.value = String(draft[key]);
    input.addEventListener("input", () => {
      draft[key] = Number(input.value);
    });
    holder.append(row(label, "", input));
  }

  const save = document.createElement("button");
  save.className = "button";
  save.textContent = t("Save");
  save.addEventListener("click", async () => {
    await invoke("theme_apply", { theme: draft, persist: true }).catch(() => {});
    holder.replaceChildren(save);
    holder.firstChild.remove();
    holder.append(editButton(holder));
  });

  const reset = document.createElement("button");
  reset.className = "button";
  reset.textContent = t("Cancel");
  reset.addEventListener("click", () => holder.replaceChildren(editButton(holder)));

  const actions = document.createElement("div");
  actions.className = "card-actions";
  actions.append(save, reset);
  holder.append(actions);

  // live preview while editing: applied but not written to theme.json yet
  const preview = () => invoke("theme_apply", { theme: draft, persist: false }).catch(() => {});
  holder.addEventListener("input", preview);
}

function editButton(holder) {
  const edit = document.createElement("button");
  edit.className = "button";
  edit.textContent = t("Edit");
  edit.addEventListener("click", () => openThemeEditor(holder));
  return edit;
}

function toggle(on, onChange) {
  const button = document.createElement("button");
  button.className = "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  button.addEventListener("click", async () => {
    const next = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(next));
    await onChange(next);
  });
  return button;
}

/** Shared pill switch, so no row falls back to a bare checkbox. */
function switchEl(checked, onChange) {
  const button = document.createElement("button");
  button.className = "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(Boolean(checked)));
  button.addEventListener("click", () => {
    const next = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(next));
    onChange(next);
  });
  return button;
}

async function boot() {
  let settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Settings");

  const pages = {
    history: "http://tauri.localhost/pages/history.html",
    extensions: "http://tauri.localhost/pages/extensions.html",
  };
  navItem("⟲", "History", false, pages.history);
  navItem("☆", "Bookmarks", false, null);
  navItem("⧉", "Extensions", false, pages.extensions);
  navItem("⚙", "Settings", true, null);

  const head = document.createElement("h2");
  head.textContent = t("Settings");
  main.append(head);

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
  card.append(row("Page zoom", "Applied to newly opened pages", zoomSelect));

  card.append(row("Appearance & theme", "", themePreview()));

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
  startCard.append(row("Start page", "Opened at launch", startUrl));
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
  themeCard.append(themeText, themeRow);
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

  const GESTURE_ACTIONS = {
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
  const PATTERNS = [
    ["U", "↑"],
    ["D", "↓"],
    ["L", "←"],
    ["R", "→"],
    ["UD", "↑↓"],
    ["DU", "↓↑"],
    ["LR", "←→"],
    ["RL", "→←"],
    ["UR", "↑→"],
    ["DL", "↓←"],
    ["UL", "↑←"],
    ["DR", "↓→"],
  ];
  const DEFAULTS_GESTURE = {
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
  const gestureMap = { ...DEFAULTS_GESTURE, ...(settings.gesture_map || {}) };

  const gestureCard = document.createElement("div");
  gestureCard.className = "card";
  gestureCard.style.maxWidth = "720px";

  gestureCard.append(
    tabToggle("Enable mouse gestures", "Right button drag", settings.gestures, "gestures"),
    tabToggle("Show the trail", "Draw the path while dragging", settings.gesture_trail, "gesture_trail"),
  );

  const color = document.createElement("input");
  color.type = "color";
  color.value = settings.gesture_color || "#4c8dff";
  color.style.width = "64px";
  color.style.height = "28px";
  color.style.border = "1px solid #3c4043";
  color.style.borderRadius = "6px";
  color.style.background = "transparent";
  color.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_color: color.value } }).catch(() => {});
  });
  gestureCard.append(row(t("Trail colour"), "", color));

  const width = document.createElement("input");
  width.type = "range";
  width.min = "1";
  width.max = "10";
  width.value = String(settings.gesture_width ?? 3);
  width.style.width = "220px";
  const widthValue = document.createElement("span");
  widthValue.className = "card-note";
  widthValue.textContent = width.value;
  width.addEventListener("input", () => {
    widthValue.textContent = width.value;
  });
  width.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_width: Number(width.value) } }).catch(() => {});
  });
  const widthHolder = document.createElement("div");
  widthHolder.style.display = "flex";
  widthHolder.style.gap = "10px";
  widthHolder.style.alignItems = "center";
  widthHolder.append(width, widthValue);
  gestureCard.append(row(t("Trail width"), "", widthHolder));

  const threshold = document.createElement("input");
  threshold.type = "range";
  threshold.min = "10";
  threshold.max = "120";
  threshold.value = String(settings.gesture_threshold ?? 34);
  threshold.style.width = "220px";
  const thresholdValue = document.createElement("span");
  thresholdValue.className = "card-note";
  thresholdValue.textContent = threshold.value;
  threshold.addEventListener("input", () => {
    thresholdValue.textContent = threshold.value;
  });
  threshold.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { gesture_threshold: Number(threshold.value) },
    }).catch(() => {});
  });
  const thresholdHolder = document.createElement("div");
  thresholdHolder.style.display = "flex";
  thresholdHolder.style.gap = "10px";
  thresholdHolder.style.alignItems = "center";
  thresholdHolder.append(threshold, thresholdValue);
  gestureCard.append(row(t("Threshold"), "Pixels before a direction counts", thresholdHolder));

  const grid = document.createElement("div");
  grid.style.display = "grid";
  grid.style.gridTemplateColumns = "repeat(2, 1fr)";
  grid.style.gap = "10px";
  grid.style.padding = "14px 18px";

  for (const [pattern, glyph] of PATTERNS) {
    const cell = document.createElement("div");
    cell.style.display = "flex";
    cell.style.alignItems = "center";
    cell.style.gap = "8px";

    const drawn = document.createElement("span");
    drawn.textContent = glyph;
    drawn.style.width = "30px";
    drawn.style.color = "#8ab4f8";
    drawn.style.fontSize = "13px";

    const select = document.createElement("select");
    select.className = "button";
    select.style.flex = "1";
    for (const [id, label] of Object.entries(GESTURE_ACTIONS)) {
      const option = document.createElement("option");
      option.value = id;
      option.textContent = t(label);
      if ((gestureMap[pattern] || "none") === id) option.selected = true;
      select.append(option);
    }
    select.addEventListener("change", async () => {
      await invoke("settings_set", {
        patch: { gesture_map: { ...gestureMap, [pattern]: select.value } },
      }).catch(() => {});
    });

    cell.append(drawn, select);
    grid.append(cell);
  }
  gestureCard.append(grid);

  const blocked = document.createElement("input");
  blocked.className = "hex";
  blocked.style.width = "420px";
  blocked.placeholder = "example.com, another.com";
  blocked.value = settings.gesture_blocked || "";
  blocked.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_blocked: blocked.value } }).catch(() => {});
  });
  gestureCard.append(row(t("Disable on these hosts"), "Comma separated", blocked));

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
  const dragHead = document.createElement("h2");
  dragHead.textContent = t("Super drag");
  dragHead.style.margin = "18px 0 10px";

  const dragCard = document.createElement("div");
  dragCard.className = "card";
  dragCard.style.maxWidth = "720px";
  const dragSwitch = switchEl(settings.super_drag !== false, async (next) => {
    await invoke("settings_set", { patch: { super_drag: next } }).catch(() => {});
  });
  const hintSwitch = switchEl(settings.drag_hint !== false, async (next) => {
    await invoke("settings_set", { patch: { drag_hint: next } }).catch(() => {});
  });

  const dragThreshold = document.createElement("input");
  dragThreshold.type = "range";
  dragThreshold.min = "8";
  dragThreshold.max = "120";
  dragThreshold.value = String(settings.drag_threshold ?? 28);
  dragThreshold.style.width = "220px";
  const dragValue = document.createElement("span");
  dragValue.className = "card-note";
  dragValue.textContent = dragThreshold.value;
  dragThreshold.addEventListener("input", () => {
    dragValue.textContent = dragThreshold.value;
  });
  dragThreshold.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { drag_threshold: Number(dragThreshold.value) },
    }).catch(() => {});
  });
  const dragHolder = document.createElement("div");
  dragHolder.style.display = "flex";
  dragHolder.style.gap = "10px";
  dragHolder.style.alignItems = "center";
  dragHolder.append(dragThreshold, dragValue);

  dragCard.append(
    row(
      t("Enable super drag"),
      t("Drag a link to open it, drag selected text to search it"),
      dragSwitch,
    ),
    row(t("Show the drag badge"), "", hintSwitch),
    row(t("Threshold"), "", dragHolder),
  );

  const DRAG_ACTIONS = {
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
  const DRAG_DEFAULTS = { L: "open", R: "search", U: "copy", D: "save" };
  const dragMap = { ...DRAG_DEFAULTS, ...(settings.drag_map || {}) };
  const dragGrid = document.createElement("div");
  dragGrid.style.display = "grid";
  dragGrid.style.gridTemplateColumns = "repeat(2, 1fr)";
  dragGrid.style.gap = "10px";
  dragGrid.style.padding = "14px 18px";

  for (const [dir, glyph] of [["U", "↑"], ["D", "↓"], ["L", "←"], ["R", "→"]]) {
    const cell = document.createElement("div");
    cell.style.display = "flex";
    cell.style.alignItems = "center";
    cell.style.gap = "8px";
    const drawn = document.createElement("span");
    drawn.textContent = glyph;
    drawn.style.width = "30px";
    drawn.style.color = "#8ab4f8";
    const select = document.createElement("select");
    select.className = "button";
    select.style.flex = "1";
    for (const [id, label] of Object.entries(DRAG_ACTIONS)) {
      const option = document.createElement("option");
      option.value = id;
      option.textContent = t(label);
      if ((dragMap[dir] || "none") === id) option.selected = true;
      select.append(option);
    }
    select.addEventListener("change", async () => {
      await invoke("settings_set", {
        patch: { drag_map: { ...dragMap, [dir]: select.value } },
      }).catch(() => {});
    });
    cell.append(drawn, select);
    dragGrid.append(cell);
  }
  dragCard.append(dragGrid);

  const gesturesHead = document.createElement("h2");
  gesturesHead.textContent = t("Mouse gestures");
  gesturesHead.style.margin = "18px 0 10px";
  main.append(tabsCard, dragHead, dragCard, gesturesHead, gestureCard);

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

  const keysHead = document.createElement("h2");
  keysHead.textContent = t("Shortcuts");
  keysHead.style.margin = "18px 0 10px";
  main.append(keysHead);

  const keysCard = document.createElement("div");
  keysCard.className = "card";
  keysCard.style.maxWidth = "720px";

  const ACTIONS = {
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
  const DEFAULTS = {
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
  const current = { ...DEFAULTS, ...(settings.shortcuts || {}) };
  const saved = { ...(settings.shortcuts || {}) };

  const conflict = document.createElement("div");
  conflict.className = "card-note";

  const checkConflicts = () => {
    const seen = new Map();
    const dupes = new Set();
    for (const [action, combo] of Object.entries(current)) {
      if (!combo) continue;
      if (seen.has(combo)) dupes.add(combo);
      seen.set(combo, action);
    }
    conflict.textContent = dupes.size
      ? t("These combos are used twice") + ": " + [...dupes].join(", ")
      : "";
  };

  for (const [action, label] of Object.entries(ACTIONS)) {
    const input = document.createElement("input");
    input.className = "hex";
    input.style.width = "160px";
    input.value = current[action] || "";
    input.placeholder = DEFAULTS[action] || "";
    input.addEventListener("change", async () => {
      const combo = input.value.trim().toLowerCase();
      current[action] = combo;
      if (combo && combo !== DEFAULTS[action]) saved[action] = combo;
      else delete saved[action];
      checkConflicts();
      await invoke("settings_set", { patch: { shortcuts: saved } }).catch(() => {});
    });
    keysCard.append(row(t(label), DEFAULTS[action] || "", input));
  }

  keysCard.append(conflict);
  main.append(keysCard);
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

  const toolbarHead = document.createElement("h2");
  // Folds into Appearance above, so the nav has one entry like other browsers.
  toolbarHead.textContent = "";
  toolbarHead.style.margin = "18px 0 10px";
  main.append(toolbarHead);

  const toolbarCard = document.createElement("div");
  toolbarCard.className = "card";
  toolbarCard.style.maxWidth = "720px";

  const NAMES = {
    downloads: "Downloads",
    translate: "Translate",
    zoom: "Zoom",
    find: "Find in page",
    bookmark: "Bookmark",
    extensions: "Extensions",
  };
  const enabled = new Set(settings.toolbar || []);
  for (const [id, label] of Object.entries(NAMES)) {
    toolbarCard.append(
      row(
        label,
        "",
        toggle(enabled.has(id), async (value) => {
          const next = new Set(settings.toolbar || []);
          if (value) next.add(id);
          else next.delete(id);
          settings.toolbar = [...next];
          await invoke("settings_set", { patch: { toolbar: [...next] } }).catch(() => {});
        }),
      ),
    );
  }


  main.append(card);
  main.append(toolbarHead, toolbarCard);
}

/**
 * The page is built as a flat list of h2 + card pairs; this folds them into
 * sections and puts a category list in the sidebar.
 */
function groupSections() {
  const groups = [];
  let current = null;

  for (const child of [...main.children]) {
    if (child.tagName === "H2") {
      current = { title: child.textContent, nodes: [] };
      groups.push(current);
      main.removeChild(child);
      continue;
    }
    if (!current) continue;
    current.nodes.push(child);
    main.removeChild(child);
  }

  const order = groups.filter((group) => group.nodes.length);
  const panels = [];

  for (const group of order) {
    const panel = document.createElement("section");
    panel.className = "section";
    panel.dataset.key = group.title;
    const head = document.createElement("h2");
    head.textContent = group.title;
    panel.append(head, ...group.nodes);
    main.append(panel);
    panels.push(panel);
  }

  const show = (key) => {
    for (const panel of panels) panel.hidden = panel.dataset.key !== key;
    for (const link of nav.querySelectorAll(".nav-section")) {
      link.classList.toggle("active", link.dataset.key === key);
    }
    main.scrollTop = 0;
  };

  const divider = document.createElement("div");
  divider.className = "nav-divider";
  nav.append(divider);

  const preferred = [
    "Startup",
    "Performance",
    "Appearance",
    "New tab page",
    "Tab settings",
    "Super drag",
    "Mouse gestures",
    "Shortcuts",
    "Backup",
    "Default browser",
    "Clear browsing data",
  ];
  const rank = (key) => {
    const index = preferred.findIndex((name) => name === key || t(name) === key);
    return index < 0 ? 99 : index;
  };
  panels.sort((a, b) => rank(a.dataset.key) - rank(b.dataset.key));
  main.append(...panels);

  for (const panel of panels) {
    const link = document.createElement("div");
    link.className = "nav-item nav-section";
    link.dataset.key = panel.dataset.key;
    const glyph = document.createElement("span");
    glyph.textContent = "·";
    const text = document.createElement("span");
    text.textContent = panel.dataset.key;
    link.append(glyph, text);
    link.addEventListener("click", () => show(panel.dataset.key));
    nav.append(link);
  }

  const wanted = decodeURIComponent((location.hash || "").replace(/^#/, ""));
  const match = panels.find((panel) => panel.dataset.key === wanted);
  if (panels.length) show(match ? match.dataset.key : panels[0].dataset.key);
}

boot().then(() => {
  groupSections();
});
