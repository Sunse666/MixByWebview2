const { invoke } = window.__TAURI__.core;
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

    el.append(icon, label, close);
    el.addEventListener("mousedown", (event) => {
      if (event.target.closest("button")) return;
      invoke("tab_select", { id: tab.id });
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

const TOOLBAR_ITEMS = {
  translate: { glyph: "\u6587", label: "Translate this page", where: "inside", run: () => invoke("translate_page", { target: null }) },
  zoom: { glyph: "\u2315", label: "Zoom", where: "inside", run: () => invoke("overlay_toggle", { kind: "zoom" }) },
  find: { glyph: "\u2315", label: "Find in page", where: "outside", run: () => invoke("overlay_toggle", { kind: "find" }) },
  bookmark: { glyph: "\u2606", label: "Bookmark this page", where: "inside", run: () => invoke("bookmark_toggle") },
  extensions: { glyph: "\u25a4", label: "Extensions", where: "outside", run: () => invoke("overlay_open", { kind: "app_menu&page=extensions" }) },
  downloads: { glyph: "\u2193", label: "Downloads", where: "outside", run: () => invoke("open_dir", { which: "downloads" }) },
};

async function renderToolbar(extensions) {
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
    if (id === "bookmark") {
      button.textContent = bookmarked ? "\u2605" : "\u2606";
    } else if (id === "zoom") {
      // Chrome shows the percentage once the page is not at 100%.
      const level = await invoke("zoom_get").catch(() => 1);
      const pct = Math.round(level * 100);
      button.textContent = pct === 100 ? spec.glyph : `${pct}%`;
      button.title = `Zoom ${pct}%`;
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
    // Extension icons sit between the puzzle and the download button.
    const at = outer.findIndex((node) => node.title === TOOLBAR_ITEMS.downloads.label);
    if (at >= 0) outer.splice(at, 0, button);
    else outer.push(button);
  }

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

function syncUrl() {
  if (document.activeElement === urlInput) return;
  const active = tabs.find((tab) => tab.id === selected);
  const next = active ? active.url : "";
  if (next && urlInput.value !== next) urlInput.value = next;
}

function setTabs(payload) {
  tabs = payload.tabs || [];
  selected = payload.selected || null;
  renderTabs();
  const active = tabs.find((tab) => tab.id === selected);
  if (active) document.title = active.title || "Mix";
  syncAddressIcon(active);
  syncUrl();
}

const report = (message) => invoke("ui_log", { message }).catch(() => {});

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

  // Listeners first: the backend may emit while this page is still starting.
  await listen("tabs:changed", (event) => {
    setTabs(event.payload);
    invoke("ext_list").then((list) => renderToolbar(list)).catch(() => {});
  });
  await listen("bookmarks:changed", async () => {
    renderBookmarks(await invoke("bookmark_list"));
    renderToolbar(await invoke("ext_list"));
  });
  await listen("ext:changed", async () => renderToolbar(await invoke("ext_list")));
  await listen("settings:changed", async () => {
    settings = await invoke("settings_get");
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
    invoke("tab_navigate", { url: urlInput.value }).catch(flagBadUrl);
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

  urlInput.addEventListener("focus", () => feedSuggest(urlInput.value));
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
      invoke("tab_navigate", { url: urlInput.value }).catch(flagBadUrl);
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