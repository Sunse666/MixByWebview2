const { invoke } = window.__TAURI__.core;
import { setLanguage, t } from "./i18n.js";
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const urlInput = document.getElementById("url");
const tabStrip = document.getElementById("tabs");

let tabs = [];
let dragging = null;
let settings = { show_bookmarks: false };
let draggingBookmark = null;
let selected = null;
let hidePill = null;
const downloadPill = document.getElementById("download");

function renderDownloads(tasks) {
  const latest = tasks && tasks.length ? tasks[tasks.length - 1] : null;
  if (!latest) {
    downloadPill.hidden = true;
    return;
  }
  downloadPill.hidden = false;
  downloadPill.dataset.id = String(latest.id);
  downloadPill.title = latest.path || latest.url;
  if (latest.state === "started") {
    const mb = (latest.bytes || 0) / (1024 * 1024);
    downloadPill.textContent = mb > 0.05
      ? `\u2193 ${mb.toFixed(1)} MB ${latest.file_name}`
      : "\u2193 " + latest.file_name;
  } else if (latest.state === "finished") {
    downloadPill.textContent = "OK " + latest.file_name;
  } else {
    downloadPill.textContent = "X " + latest.file_name;
  }
  if (hidePill) clearTimeout(hidePill);
  if (latest.state !== "started") {
    hidePill = setTimeout(() => {
      downloadPill.hidden = true;
    }, 8000);
  }
}

function applyTheme(theme) {
  const s = root.style;
  s.setProperty("--bg", theme.colors.bg);
  s.setProperty("--panel", theme.colors.chrome);
  s.setProperty("--accent", theme.colors.accent);
  s.setProperty("--text", theme.colors.text);
  s.setProperty("--border", theme.colors.border);
  s.setProperty("--radius", `${theme.radius}px`);
  s.setProperty("--font-size", `${theme.font_size}px`);
  s.setProperty("--tab", theme.colors.tab);
  s.setProperty("--tab-text", theme.colors.tab_text);
  s.setProperty("--url-bg", theme.colors.url_bg);
  s.setProperty("--hover", theme.colors.hover);
  s.setProperty("--font-family", theme.font_family);
  s.setProperty("--tab-radius", `${theme.tab_radius}px`);
}

// Rust calls this after every theme change, including live preview.
window.__mixApplyTheme = applyTheme;

function renderTabs() {
  const nodes = tabs.map((tab) => {
    const el = document.createElement("div");
    const classes = ["tab"];
    if (tab.id === selected) classes.push("active");
    if (!tab.alive) classes.push("suspended");
    if (tab.incognito) classes.push("incognito");
    if (tab.pinned) classes.push("pinned");
    el.className = classes.join(" ");
    el.title = tab.url;

    const label = document.createElement("span");
    const icon = document.createElement(tab.loading ? "span" : tab.icon ? "img" : "span");
    icon.className = tab.loading ? "tab-icon spinner" : "tab-icon";
    if (!tab.loading && tab.icon) {
      icon.src = tab.icon;
      icon.alt = "";
      icon.addEventListener("error", () => icon.remove());
    } else if (!tab.loading) {
      icon.textContent = "";
    }

    label.className = "tab-title";
    label.textContent = tab.title || "New tab";

    const close = document.createElement("button");
    close.className = "tab-close";
    close.textContent = "x";
    close.title = "Close tab";
    close.addEventListener("mousedown", (event) => event.stopPropagation());
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      invoke("tab_close", { id: tab.id });
    });

    if (tab.pinned) {
      // A pinned tab shows a glyph instead of the close button.
      const pin = document.createElement("span");
      pin.className = "tab-pin";
      pin.textContent = "\u25C6";
      pin.title = "Pinned";
      el.append(icon, label, pin);
    } else {
      el.append(icon, label, close);
    }
    el.addEventListener("mousedown", (event) => {
      if (event.target.closest("button")) return;
      invoke("tab_select", { id: tab.id });
    });

    // Middle click pins or releases a tab.
    el.addEventListener("auxclick", (event) => {
      if (event.button !== 1) return;
      event.preventDefault();
      invoke("tab_pin", { id: tab.id, pinned: !tab.pinned }).catch(() => {});
      setTimeout(() => invoke("tab_list").catch(() => {}), 60);
    });

    el.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      invoke("tab_pin", { id: tab.id, pinned: !tab.pinned }).catch(() => {});
      setTimeout(() => invoke("tab_list").catch(() => {}), 60);
    });

    // Drag to reorder. The strip only sends the index, Rust does the move.
    el.draggable = true;
    el.addEventListener("dragstart", (event) => {
      dragging = tab.id;
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", tab.url);
      el.classList.add("dragging");
    });
    el.addEventListener("dragend", () => {
      dragging = null;
      el.classList.remove("dragging");
      for (const pill of tabStrip.querySelectorAll(".tab")) pill.classList.remove("drop-target");
    });
    el.addEventListener("dragover", (event) => {
      if (!dragging || dragging === tab.id) return;
      event.preventDefault();
      el.classList.add("drop-target");
    });
    el.addEventListener("dragleave", () => el.classList.remove("drop-target"));
    el.addEventListener("drop", (event) => {
      event.preventDefault();
      el.classList.remove("drop-target");
      const source = dragging;
      if (!source || source === tab.id) return;
      const to = tabs.findIndex((item) => item.id === tab.id);
      invoke("tab_move", { id: source, to }).catch(() => {});
    });

    return el;
  });
  tabStrip.replaceChildren(...nodes);
}

// Whether the current tab shows translated text; the backend is the source of
// truth so navigation cannot leave a stale "translated" state behind.
let translateOn = false;

async function runTranslate() {
  await invoke("translate_page_inplace").catch(() => {});
  for (let attempt = 0; attempt < 40; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 400));
    const [count, error] = await invoke("translate_status").catch(() => [0, null]);
    if (error) return false;
    if (count > 0) return true;
  }
  return false;
}

async function toggleTranslate() {
  if (!selected) return;

  const already = await invoke("translate_state").catch(() => false);
  if (already) {
    await invoke("translate_restore").catch(() => {});
    translateOn = false;
    renderToolbar(currentExtensions);
    return;
  }

  translateOn = await runTranslate();
  // Pages that render late have nothing to collect yet; one retry covers it.
  if (!translateOn) {
    await new Promise((resolve) => setTimeout(resolve, 700));
    translateOn = await runTranslate();
  }
  translateOn = await invoke("translate_state").catch(() => translateOn);
  renderToolbar(currentExtensions);
}

// Top level menus; each opens its own dropdown in an overlay window because the
// strip is too short to hold a popup.
// When the input is not an address, the address bar searches the web.
const SEARCH_ENGINES = {
  baidu: "https://www.baidu.com/s?wd=",
  bing: "https://www.bing.com/search?q=",
  google: "https://www.google.com/search?q=",
  ddg: "https://duckduckgo.com/?q=",
};

function looksLikeAddress(text) {
  const value = (text || "").trim();
  if (!value) return null;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return value;
  if (value.startsWith("mix://")) {
    const page = value.slice(6).split("/")[0];
    return "http://tauri.localhost/pages/" + page + ".html";
  }
  if (value.includes(" ") || !value.includes(".")) return null;
  return "https://" + value;
}

function addressFor(text) {
  const direct = looksLikeAddress(text);
  if (direct) return direct;
  const base = SEARCH_ENGINES[settings.search_engine] || SEARCH_ENGINES.baidu;
  return base + encodeURIComponent((text || "").trim());
}

// Command palette: one search box over actions, history and bookmarks.
function openPalette() {
  const back = document.createElement("div");
  back.className = "palette-backdrop";

  const box = document.createElement("div");
  box.className = "palette";

  const input = document.createElement("input");
  input.className = "palette-input";
  input.placeholder = t("Type a command or a site");

  const list = document.createElement("div");
  list.className = "palette-list";

  box.append(input, list);
  back.append(box);
  document.body.append(back);
  input.focus();

  const actions = MENU_BAR.flatMap((group) =>
    (group.items || []).map((item) => ({
      label: group.name + " · " + t(item.label || item.name || ""),
      run: () => item.run && item.run(),
    })),
  );

  const close = () => back.remove();

  const paint = async (query) => {
    const text = (query || "").trim().toLowerCase();
    list.replaceChildren();

    const matchedActions = actions
      .filter((action) => !text || action.label.toLowerCase().includes(text))
      .slice(0, 8);

    for (const action of matchedActions) {
      const row = document.createElement("div");
      row.className = "palette-row";
      row.textContent = action.label;
      row.addEventListener("click", () => {
        close();
        action.run();
      });
      list.append(row);
    }

    if (text.length >= 2) {
      const history = (await invoke("history_list", { limit: 200 }).catch(() => [])) || [];
      const seen = new Set();
      const sites = history
        .filter((visit) => !seen.has(visit.url) && seen.add(visit.url))
        .filter((visit) => (visit.url + " " + (visit.title || "")).toLowerCase().includes(text))
        .slice(0, 8);

      for (const visit of sites) {
        const row = document.createElement("div");
        row.className = "palette-row";
        row.textContent = (visit.title || visit.url) + " — " + visit.url;
        row.addEventListener("click", () => {
          close();
          invoke("tab_navigate", { url: visit.url }).catch(() => {});
        });
        list.append(row);
      }

      const bookmarks = (await invoke("bookmark_list").catch(() => [])) || [];
      for (const item of bookmarks
        .filter((entry) => (entry.url + " " + entry.title).toLowerCase().includes(text))
        .slice(0, 5)) {
        const row = document.createElement("div");
        row.className = "palette-row";
        row.textContent = "☆ " + item.title + " — " + item.url;
        row.addEventListener("click", () => {
          close();
          invoke("tab_navigate", { url: item.url }).catch(() => {});
        });
        list.append(row);
      }
    }

    const first = list.firstElementChild;
    if (first) first.classList.add("active");
  };

  input.addEventListener("input", () => paint(input.value));
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") close();
    if (event.key === "Enter") {
      const active = list.querySelector(".palette-row.active") || list.firstElementChild;
      if (active) active.click();
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const rows = [...list.querySelectorAll(".palette-row")];
      if (!rows.length) return;
      const current = rows.findIndex((row) => row.classList.contains("active"));
      const next = event.key === "ArrowDown"
        ? (current + 1) % rows.length
        : (current <= 0 ? rows.length - 1 : current - 1);
      rows.forEach((row) => row.classList.remove("active"));
      rows[next].classList.add("active");
      rows[next].scrollIntoView({ block: "nearest" });
    }
  });
  back.addEventListener("mousedown", (event) => {
    if (event.target === back) close();
  });

  paint("");
}

const MENU_BAR = [
  { id: "file", label: "File" },
  { id: "edit", label: "Edit" },
  { id: "view", label: "View" },
  { id: "history", label: "History" },
  { id: "bookmarks", label: "Bookmarks" },
  { id: "tools", label: "Tools" },
  { id: "help", label: "Help" },
];

function renderMenuBar() {
  const bar = document.getElementById("menubar");
  if (!bar) return;
  bar.hidden = !(settings.menu_bar !== false);
  bar.replaceChildren(
    ...MENU_BAR.map((entry) => {
      const button = document.createElement("button");
      button.textContent = t(entry.label);
      button.addEventListener("click", () => {
        const box = button.getBoundingClientRect();
        invoke("overlay_open_at", {
          kind: `menubar_menu&group=${entry.id}`,
          x: Math.round(box.left),
          y: Math.round(box.bottom + 2),
        }).catch(() => {});
      });
      return button;
    }),
  );
}

const TOOLBAR_ITEMS = {
  translate: { glyph: "\u6587", label: "Translate this page", where: "inside", run: () => toggleTranslate() },
  zoom: { glyph: "\u2315", label: "Zoom", where: "inside", run: () => invoke("overlay_toggle", { kind: "zoom" }) },
  find: { glyph: "\u2315", label: "Find in page", where: "outside", run: () => invoke("overlay_toggle", { kind: "find" }) },
  bookmark: { glyph: "\u2606", label: "Bookmark this page", where: "inside", run: () => invoke("bookmark_toggle") },
  extensions: { glyph: "\u25a4", label: "Extensions", where: "outside", run: () => invoke("overlay_open", { kind: "app_menu&page=extensions" }) },
  downloads: { glyph: "\u2193", label: "Downloads", where: "outside", run: () => invoke("open_dir", { which: "downloads" }) },
};

let currentExtensions = [];

async function renderToolbar(extensions) {
  if (extensions) currentExtensions = extensions;
  const saved = new Set((await invoke("bookmark_list")).map((item) => item.url));
  const active = tabs.find((tab) => tab.id === selected);
  const bookmarked = active ? saved.has(active.url) : false;

  const inside = document.getElementById("url-actions");
  const outside = document.getElementById("toolbar");
  const inner = [];
  const outer = [];

  // Order follows Chrome: extensions puzzle first, downloads last.
  const order = ["translate", "zoom", "find", "bookmark", "extensions", "downloads"];
  for (const id of order) {
    if (!(settings.toolbar || []).includes(id)) continue;
    const spec = TOOLBAR_ITEMS[id];
    if (!spec) continue;
    const button = document.createElement("button");
    button.className = spec.where === "inside" ? "url-action" : "tool-btn";
    button.title = spec.label;
    if (id === "translate") {
      const on = translateOn;
      button.textContent = spec.glyph;
      button.title = t(on ? "Show original" : spec.label);
      button.classList.toggle("is-on", Boolean(on));
    } else if (id === "bookmark") {
      button.textContent = bookmarked ? "\u2605" : "\u2606";
    } else if (id === "zoom") {
      // Chrome shows the percentage once the page is not at 100%.
      const level = await invoke("zoom_get").catch(() => 1);
      const pct = Math.round(level * 100);
      button.textContent = pct === 100 ? spec.glyph : `${pct}%`;
      button.title = `${t("Zoom")} ${pct}%`;
    } else {
      button.textContent = spec.glyph;
    }
    button.addEventListener("click", () => spec.run().catch(() => {}));
    (spec.where === "inside" ? inner : outer).push(button);
  }

  const hidden = new Set(settings.hidden_extensions || []);
  for (const ext of (extensions || []).filter((item) => item.status === "loaded" && item.icon)) {
    if (hidden.has(ext.name)) continue;
    const button = document.createElement("button");
    button.className = "tool-btn ext-btn";
    button.title = ext.version ? `${ext.name} ${ext.version}` : ext.name;
    const img = document.createElement("img");
    img.src = ext.icon;
    img.alt = "";
    button.append(img);
    button.addEventListener("click", () =>
      invoke("overlay_open", { kind: `ext_info&name=${encodeURIComponent(ext.name)}` }),
    );
    // Extension icons come before the puzzle menu, so the menu stays rightmost.
    outer.unshift(button);
  }

  // Chrome order: every extension icon, then the puzzle, then downloads.
  const weight = (node) => {
    if (node.title === t(TOOLBAR_ITEMS.extensions.label)) return 1;
    if (node.title === t(TOOLBAR_ITEMS.downloads.label)) return 2;
    return 0;
  };
  outer.sort((left, right) => weight(left) - weight(right));
  inside.replaceChildren(...inner);
  outside.hidden = outer.length === 0;
  outside.replaceChildren(...outer);
}


function renderBookmarks(list) {
  const bar = document.getElementById("bookmarks");
  const items = list || [];
  bar.hidden = !settings.show_bookmarks;
  if (bar.hidden) return;

  bar.replaceChildren(
    ...items.map((bookmark, index) => {
      const chip = document.createElement("button");
      chip.className = "bookmark";
      chip.textContent = bookmark.title || bookmark.url;
      chip.title = bookmark.url;
      chip.draggable = true;
      chip.addEventListener("click", () => invoke("tab_new", { url: bookmark.url }));
      chip.addEventListener("dragstart", (event) => {
        draggingBookmark = bookmark.url;
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", bookmark.url);
        chip.classList.add("dragging");
      });
      chip.addEventListener("dragend", () => {
        draggingBookmark = null;
        chip.classList.remove("dragging");
      });
      chip.addEventListener("dragover", (event) => {
        if (!draggingBookmark) return;
        event.preventDefault();
        chip.classList.add("drop-target");
      });
      chip.addEventListener("dragleave", () => chip.classList.remove("drop-target"));
      chip.addEventListener("drop", (event) => {
        event.preventDefault();
        chip.classList.remove("drop-target");
        const source = draggingBookmark;
        if (!source || source === bookmark.url) return;
        invoke("bookmark_move", { from: source, to: index }).catch(() => {});
      });
      return chip;
    }),
  );

  if (!items.length) {
    const hint = document.createElement("span");
    hint.className = "bookmark-hint";
    hint.textContent = "Drag a link here to bookmark it";
    bar.append(hint);
  }
}

function syncAddressIcon(active) {
  const holder = document.getElementById("url-icon");
  if (!holder) return;
  if (active && active.icon) {
    holder.src = active.icon;
    holder.hidden = false;
  } else {
    holder.hidden = true;
  }
}

// Internal pages live on the asset host; show them the way a browser would.
function prettyUrl(url) {
  const prefix = "http://tauri.localhost/pages/";
  if (url.startsWith(prefix)) {
    return "mix://" + url.slice(prefix.length).replace(/\.html$/, "");
  }
  if (url === "about:blank") return "";
  return url;
}

function syncUrl() {
  if (document.activeElement === urlInput) return;
  const active = tabs.find((tab) => tab.id === selected);
  const next = active ? prettyUrl(active.url) : "";
  if (next && urlInput.value !== next) urlInput.value = next;
}

function setTabs(payload) {
  tabs = payload.tabs || [];
  selected = payload.selected || null;
  renderTabs();
  const active = tabs.find((tab) => tab.id === selected);
  if (active) document.title = active.title || "Mix";
  // Keep the incognito state visible in the chrome itself.
  document.body.classList.toggle("incognito", Boolean(active && active.incognito));
  syncAddressIcon(active);
  syncUrl();
}

const report = (message) => invoke("ui_log", { message }).catch(() => {});

// Translation failures are reported by the backend; a silent no-op looks broken.
let translateError = "";

function showToast(text) {
  const toast = document.getElementById("toast");
  if (!toast) return;
  toast.textContent = text;
  toast.hidden = false;
  clearTimeout(showToast.timer);
  // A failure message deserves a longer look than a success flash.
  showToast.timer = setTimeout(() => {
    toast.hidden = true;
  }, 7000);
}

window.__mixTranslateFailed = (message) => {
  translateError = String(message || "failed");
  showToast(`${t("Translate failed")}: ${t(translateError)}`);
  renderToolbar(currentExtensions);
};
report("chrome boot v2");

// Chrome draws its own menus; the WebView2 default menu does not belong on our UI.
// Edits requested by the overlay menu; the field lives in this page.
window.__mixUrlAction = (action) => {
  urlInput.focus();
  const selection = urlInput.value.slice(urlInput.selectionStart, urlInput.selectionEnd);
  const place = (text) => {
    urlInput.setRangeText(text, urlInput.selectionStart, urlInput.selectionEnd, "end");
    urlInput.dispatchEvent(new Event("input"));
  };
  if (action === "cut") {
    invoke("clipboard_write", { text: selection }).catch(() => {});
    place("");
  } else if (action === "copy") {
    invoke("clipboard_write", { text: selection }).catch(() => {});
  } else if (action === "paste") {
    invoke("clipboard_read")
      .then((text) => place(text || ""))
      .catch(() => {});
  } else if (action === "select-all") {
    urlInput.select();
  }
};

const ctxMenu = document.getElementById("ctx-menu");

function hideContextMenu() {
  if (!ctxMenu) return;
  ctxMenu.hidden = true;
  ctxMenu.replaceChildren();
}

function showContextMenu(x, y, rows) {
  if (!ctxMenu) return;
  ctxMenu.replaceChildren(
    ...rows.map((row) => {
      if (row === "-") {
        return Object.assign(document.createElement("div"), { className: "ctx-divider" });
      }
      const button = document.createElement("button");
      button.className = "ctx-item";
      button.textContent = row.label;
      button.disabled = Boolean(row.disabled);
      button.addEventListener("click", () => {
        hideContextMenu();
        row.run();
      });
      return button;
    }),
  );
  ctxMenu.hidden = false;
  // Keep the menu inside the window.
  const width = ctxMenu.offsetWidth || 180;
  const height = ctxMenu.offsetHeight || 200;
  ctxMenu.style.left = Math.min(x, window.innerWidth - width - 6) + "px";
  ctxMenu.style.top = Math.min(y, window.innerHeight - height - 6) + "px";
}

function urlEditMenu() {
  const selection = urlInput.value.slice(urlInput.selectionStart, urlInput.selectionEnd);
  const read = () => navigator.clipboard.readText();
  const place = (text) => {
    const start = urlInput.selectionStart;
    const end = urlInput.selectionEnd;
    urlInput.setRangeText(text, start, end, "end");
    urlInput.dispatchEvent(new Event("input"));
  };

  return [
    {
      label: "Cut",
      disabled: !selection,
      run: () => {
        navigator.clipboard.writeText(selection).catch(() => {});
        place("");
      },
    },
    {
      label: "Copy",
      disabled: !selection,
      run: () => navigator.clipboard.writeText(selection).catch(() => {}),
    },
    { label: "Paste", run: () => read().then(place).catch(() => {}) },
    "-",
    {
      label: "Select all",
      run: () => {
        urlInput.focus();
        urlInput.select();
      },
    },
  ];
}

function chromeMenu() {
  return [
    { label: "New tab", run: () => invoke("tab_new", { url: null }) },
    { label: "Reload", run: () => invoke("tab_reload") },
    "-",
    { label: "Settings", run: () => invoke("overlay_open", { kind: "app_menu&page=settings" }) },
  ];
}

document.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  const inField = event.target === urlInput || Boolean(event.target.closest("#url-wrap"));
  invoke("chrome_menu", {
    kind: inField ? "url" : "chrome",
    x: event.clientX,
    y: event.clientY,
  }).catch(() => {});
});

document.addEventListener("mousedown", (event) => {
  if (!event.target.closest("#ctx-menu")) hideContextMenu();
  // Anything that is not the field itself gives the keyboard back to the page.
  if (document.activeElement === urlInput && !event.target.closest("#url-wrap")) {
    urlInput.blur();
    invoke("focus_page").catch(() => {});
  }
});
window.addEventListener("blur", hideContextMenu);
window.addEventListener("resize", hideContextMenu);

// Rust calls this after saving settings: the event bus does not reach child
// webviews reliably, so the refresh is pushed in explicitly.
window.__mixSettingsChanged = async () => {
  settings = await invoke("settings_get");
  renderToolbar(await invoke("ext_list"));
  renderBookmarks(await invoke("bookmark_list"));
};

async function boot() {
  applyTheme(await invoke("theme_get"));
  settings = await invoke("settings_get");
  setLanguage(settings.language || "en");
  renderMenuBar();

  // Listeners first: the backend may emit while this page is still starting.
  await listen("tabs:changed", (event) => {
    setTabs(event.payload);
    invoke("translate_state")
      .then((on) => {
        translateOn = Boolean(on);
        return invoke("ext_list");
      })
      .then((list) => renderToolbar(list))
      .catch(() => {});
  });
  await listen("bookmarks:changed", async () => {
    renderBookmarks(await invoke("bookmark_list"));
    renderToolbar(await invoke("ext_list"));
  });
  await listen("ext:changed", async () => renderToolbar(await invoke("ext_list")));
  await listen("settings:changed", async () => {
    settings = await invoke("settings_get");
    renderMenuBar();
    renderBookmarks(await invoke("bookmark_list"));
    renderToolbar(await invoke("ext_list"));

  });
  await listen("tab:loading", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.loading = event.payload.loading;
    renderTabs();
  });
  await listen("tab:icon", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.icon = event.payload.icon;
    renderTabs();
    syncAddressIcon(tabs.find((item) => item.id === selected));
  });
  await listen("tab:title", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.title = event.payload.title;
    renderTabs();
    if (tab.id === selected) document.title = tab.title || "Mix";
  });
  await listen("tab:url", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.url = event.payload.url;
    if (tab.id === selected) syncUrl();
  });
  await listen("theme:changed", (event) => applyTheme(event.payload));
  await listen("download:changed", (event) => renderDownloads(event.payload));
  setTabs(await invoke("tab_list"));
  report(`boot tabs=${tabs.length} selected=${selected}`);

  await listen("ui:new-tab", () => invoke("tab_new", {}));
  await listen("ui:open-url", (event) => invoke("tab_new", { url: event.payload }));
  await listen("url:text", (event) => {
    urlInput.value = event.payload || "";
    urlInput.setSelectionRange(urlInput.value.length, urlInput.value.length);
  });
  await listen("url:commit", () => {
    invoke("tab_navigate", { url: addressFor(urlInput.value) }).catch(flagBadUrl);
    urlInput.blur();
  });
  await listen("url:cancel", () => {
    syncUrl();
    urlInput.blur();
  });
  await listen("ui:focus-url", () => {
    urlInput.focus();
    urlInput.select();
    // Focus events inside an unfocused webview are unreliable, so feed directly.
    if (typeof feedSuggest === "function") feedSuggest(urlInput.value);
  });
  const bar = document.getElementById("bookmarks");
  bar.addEventListener("dragover", (event) => {
    event.preventDefault();
    bar.classList.add("drop-target");
  });
  bar.addEventListener("dragleave", () => bar.classList.remove("drop-target"));
  bar.addEventListener("drop", (event) => {
    event.preventDefault();
    bar.classList.remove("drop-target");
    const url = event.dataTransfer.getData("text/plain");
    if (!url || !url.startsWith("http")) return;
    invoke("bookmark_add", { url, title: url }).catch(() => {});
  });

  renderDownloads(await invoke("download_list"));
  renderToolbar(await invoke("ext_list"));
  renderBookmarks(await invoke("bookmark_list"));

  for (const button of document.querySelectorAll("[data-window]")) {
    button.addEventListener("click", () => invoke("window_control", { action: button.dataset.window }));
  }
  for (const button of document.querySelectorAll("[data-nav]")) {
    button.addEventListener("click", () => invoke("tab_nav", { action: button.dataset.nav }));
  }
  for (const handle of document.querySelectorAll("[data-resize]")) {
    handle.addEventListener("mousedown", (event) => {
      event.preventDefault();
      invoke("window_resize", { direction: handle.dataset.resize });
    });
  }

  const drag = document.getElementById("drag");
  drag.addEventListener("mousedown", (event) => {
    if (event.target.closest("button") || event.target.closest(".tab")) return;
    invoke("window_drag");
  });
  drag.addEventListener("dblclick", (event) => {
    if (event.target.closest("button") || event.target.closest(".tab")) return;
    invoke("window_control", { action: "max" });
  });

  // Same map as the page side, so shortcuts work wherever the focus sits.
  window.addEventListener("keydown", (event) => {
    if (event.altKey && event.shiftKey && event.key.startsWith("Arrow")) {
      event.preventDefault();
      invoke("snap", { direction: event.key.replace("Arrow", "").toLowerCase() });
      return;
    }
    let action = null;
    if (event.key === "F5") action = "reload";
    else if (event.key === "F11") action = "fullscreen";
    else if (event.key === "F12") action = "devtools";
    else if (event.ctrlKey || event.metaKey) {
      // Shift turns "t" into "T"; the map below is keyed on the letter.
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      switch (key) {
        case "t": action = event.shiftKey ? "reopen_tab" : "new_tab"; break;
        case "w": action = "close_tab"; break;
        case "l": action = "focus_url"; break;
        case "r": action = "reload"; break;
        case "Tab": action = event.shiftKey ? "prev_tab" : "next_tab"; break;
        case "=":
        case "+": action = "zoom_in"; break;
        case "-": action = "zoom_out"; break;
        case "0": action = "zoom_reset"; break;
        case "p": action = "print"; break;
        case "j": action = "downloads"; break;
        case "d": action = "bookmark"; break;
        case "n": action = event.shiftKey ? "new_incognito" : "new_window"; break;
        default: action = null;
      }
    }
    if (!action) return;
    event.preventDefault();
    if (action === "focus_url") {
      urlInput.focus();
      urlInput.select();
      return;
    }
    invoke("shortcut", { action });
  }, true);

  // Address bar autocomplete: the dropdown is a separate overlay webview,
  // because the chrome strip is only 68px tall and would clip it.
  var suggestOpen = false;
  var feedSuggest = (text) => {
    if (!suggestOpen) {
      suggestOpen = true;
      const box = urlInput.getBoundingClientRect();
      invoke("suggest_open", { x: box.left, width: box.width })
        .then(() => invoke("suggest_feed", { text }))
        .catch((error) => {
          suggestOpen = false;
          report(`suggest open failed: ${error}`);
        });
      return;
    }
    invoke("suggest_feed", { text }).catch(() => {});
  };
  const closeSuggest = () => {
    if (!suggestOpen) return;
    suggestOpen = false;
    invoke("overlay_close").catch(() => {});
  };

  // The dropdown opens on typing, not on focus: an open dropdown is a separate
  // webview that would swallow the click meant to move focus to the page.
  urlInput.addEventListener("keydown", (event) => {
    if (event.key.length === 1 || event.key === "Backspace" || event.key === "Delete") {
      feedSuggest(urlInput.value);
    }
  });
  urlInput.addEventListener("input", () => feedSuggest(urlInput.value));
  urlInput.addEventListener("blur", () => closeSuggest());

  let badUrlTimer = null;
  const flagBadUrl = () => {
    urlInput.classList.add("invalid");
    urlInput.title = "Not a web address";
    if (badUrlTimer) clearTimeout(badUrlTimer);
    badUrlTimer = setTimeout(() => {
      urlInput.classList.remove("invalid");
      urlInput.title = "";
    }, 1600);
  };

  urlInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      invoke("tab_navigate", { url: addressFor(urlInput.value) }).catch(flagBadUrl);
      closeSuggest();
      urlInput.blur();
    }
    if (event.key === "Escape") {
      closeSuggest();
      syncUrl();
      urlInput.blur();
    }
  });


  downloadPill.addEventListener("click", () => {
    const id = Number(downloadPill.dataset.id);
    if (!Number.isNaN(id)) invoke("download_open_dir", { id });
  });

  document.getElementById("theme-btn").addEventListener("click", () => invoke("overlay_toggle", { kind: "app_menu" }));
  // Middle click closes a tab, like every other browser.
  document
    .getElementById("tabs")
    .addEventListener("auxclick", (event) => {
      if (event.button !== 1) return;
      const pill = event.target.closest(".tab");
      if (pill && pill.dataset.id) invoke("tab_close", { id: pill.dataset.id });
    });
  document.getElementById("tab-new").addEventListener("click", () => invoke("tab_new", {}));

  // The address bar is not focused on purpose: it would open the suggestion
  // dropdown over the page on every launch. Ctrl+L focuses it.
}

boot().catch((error) => {
  console.error("boot failed:", error);
  report(`boot failed: ${error && error.message ? error.message : error}`);
});