const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const fields = {
  bg: document.getElementById("bg"),
  chrome: document.getElementById("chrome"),
  tab: document.getElementById("tab"),
  tab_text: document.getElementById("tab-text"),
  text: document.getElementById("text"),
  accent: document.getElementById("accent"),
  url_bg: document.getElementById("url-bg"),
  hover: document.getElementById("hover"),
  border: document.getElementById("border"),
};
const radius = document.getElementById("radius");
const radiusOut = document.getElementById("radius-out");
const tabRadius = document.getElementById("tab-radius");
const tabRadiusOut = document.getElementById("tab-radius-out");
const fontSize = document.getElementById("font-size");
const fontOut = document.getElementById("font-out");
const fontFamily = document.getElementById("font-family");

const PRESETS = {
  green: { bg: "#1F2020", chrome: "#1F2020", accent: "#00FF00", text: "#E8E8E8", border: "#2E3131", radius: 15 },
  dark: { bg: "#111315", chrome: "#181B1E", accent: "#8AB4F8", text: "#DADCE0", border: "#2A2F34", radius: 8 },
  light: { bg: "#F5F6F7", chrome: "#FFFFFF", accent: "#1A73E8", text: "#202124", border: "#D6D9DC", radius: 10 },
};

const report = (message) => invoke("ui_log", { message }).catch(() => {});
report(`overlay boot hash=${location.hash} size=${window.innerWidth}x${window.innerHeight}`);

const rawHash = location.hash.replace("#", "") || "settings";
const kind = rawHash.split("&")[0];
const params = new Map(
  rawHash
    .split("&")
    .slice(1)
    .map((pair) => pair.split("="))
    .map(([key, value]) => [key, decodeURIComponent(value || "")]),
);
let current = null;

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

// Rust pushes every change here too, so preview and chrome stay in sync.
window.__mixApplyTheme = applyTheme;

function render(theme) {
  applyTheme(theme);
  for (const [key, input] of Object.entries(fields)) input.value = theme.colors[key];
  radius.value = String(Math.round(theme.radius));
  radiusOut.textContent = `${Math.round(theme.radius)}px`;
  tabRadius.value = String(Math.round(theme.tab_radius));
  tabRadiusOut.textContent = `${Math.round(theme.tab_radius)}px`;
  fontSize.value = String(Math.round(theme.font_size));
  fontOut.textContent = `${Math.round(theme.font_size)}px`;
  fontFamily.value = theme.font_family;
}

async function preview() {
  const colors = { ...current.colors };
  for (const [key, input] of Object.entries(fields)) colors[key] = input.value;
  const next = {
    ...current,
    colors,
    radius: Number(radius.value),
    tab_radius: Number(tabRadius.value),
    font_size: Number(fontSize.value),
    font_family: fontFamily.value,
  };
  current = await invoke("theme_apply", { theme: next, persist: false });
  radiusOut.textContent = `${Math.round(current.radius)}px`;
  tabRadiusOut.textContent = `${Math.round(current.tab_radius)}px`;
  fontOut.textContent = `${Math.round(current.font_size)}px`;
}

async function close() {
  await invoke("overlay_close");
}

const menu = document.getElementById("menu");
const appMenu = document.getElementById("app-menu");
const themePanel = document.getElementById("theme-panel");
const permissionPanel = document.getElementById("permission-panel");
const autostartBox = document.getElementById("autostart");
const restoreBox = document.getElementById("restore");

async function bootAppMenu() {
  appMenu.hidden = false;
  themePanel.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const close = () => invoke("overlay_close");
  const info = await invoke("app_info");

  const clear = () => {
    while (appMenu.firstChild) appMenu.removeChild(appMenu.firstChild);
  };

  const divider = () => appMenu.append(Object.assign(document.createElement("div"), { className: "divider" }));

  const header = (title) => {
    const bar = document.createElement("div");
    bar.className = "menu-header";
    const back = document.createElement("button");
    back.className = "back";
    back.textContent = "\u2039";
    back.title = "Back";
    back.addEventListener("click", () => render("root"));
    const label = document.createElement("span");
    label.textContent = title;
    bar.append(back, label);
    appMenu.append(bar);
  };

  const row = (glyph, label, hint, run, options = {}) => {
    const button = document.createElement("button");
    button.className = "row" + (options.danger ? " danger" : "") + (options.muted ? " muted" : "");

    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;

    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label;

    const key = document.createElement("span");
    key.className = "hint";
    key.textContent = hint || (options.page ? "\u203a" : "");

    button.append(icon, text, key);
    if (run) button.addEventListener("click", run);
    appMenu.append(button);
    return button;
  };

  const act = (fn) => async () => {
    await fn();
    await close();
  };

  const pages = {
    root: async () => {
      row("+", "New tab", "Ctrl+T", act(() => invoke("tab_new", {})));
      row("\u25a2", "New window", "Ctrl+N", act(() => invoke("window_new")));
      row("\u25d1", "New incognito tab", "Ctrl+Shift+N", act(() => invoke("tab_new", { incognito: true })));
      row("\u21ba", "Reopen closed tab", "Ctrl+Shift+T", async () => {
        await invoke("tab_reopen");
        await close();
      });
      row("\u2193", "Downloads", "Ctrl+J", act(() => invoke("open_dir", { which: "downloads" })));
      divider();

      // Zoom strip: - 100% +
      const zoomRow = document.createElement("div");
      zoomRow.className = "row zoom-row";
      const zoomIcon = document.createElement("span");
      zoomIcon.className = "ico";
      zoomIcon.textContent = "\u2315";
      const minus = document.createElement("button");
      minus.className = "step";
      minus.textContent = "\u2212";
      const level = document.createElement("button");
      level.className = "level";
      level.title = "Reset zoom";
      const plus = document.createElement("button");
      plus.className = "step";
      plus.textContent = "+";
      let zoom = await invoke("zoom_get");
      const paint = (value) => {
        zoom = value;
        level.textContent = Math.round(value * 100) + "%";
      };
      paint(zoom);
      minus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: -0.1 })));
      plus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 0.1 })));
      level.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 1 - zoom })));
      const controls = document.createElement("div");
      controls.className = "zoom-controls";
      controls.append(minus, level, plus);
      zoomRow.append(zoomIcon, controls);
      appMenu.append(zoomRow);

      row("\u25a3", "Full screen", "F11", act(() => invoke("window_fullscreen", { enabled: null })));
      divider();
      row("\u2399", "Print", "Ctrl+P", act(() => invoke("tab_print")));
      row("\u4e16", "Translate this page", "", () => invoke("overlay_toggle", { kind: "translate" }));
      row("\u21aa", "Open in the full translator", "", async () => {
        await invoke("translate_page", { target: null }).catch((error) => report(String(error)));
        await close();
      });
      row("\u2717", "Clear browsing data", "Ctrl+Shift+Del", async () => {
        await invoke("app_clear_data");
        await close();
      });
      divider();
      row("\u2606", "Bookmark this page", "Ctrl+D", async () => {
        const added = await invoke("bookmark_toggle");
        report("bookmark toggled -> " + added);
        await close();
      });
      row("\u2606", "Bookmarks", "", () => render("bookmarks"), { page: true });
      row("\u21ba", "History", "", () => render("history"), { page: true });
      divider();
      row("\u25a4", "Extensions", "", () => render("extensions"), { page: true });
      row("\u2699", "Userscripts", "", () => render("scripts"), { page: true });
      row("\u2261", "More tools", "", () => render("tools"), { page: true });
      divider();
      row("\u2699", "Settings", "", () => render("settings"), { page: true });
      row("\u003f", "Help", "", () => render("help"), { page: true });

      const about = document.createElement("div");
      about.className = "about";
      about.textContent = "Mix " + info.version + " (" + info.git + ")";
      appMenu.append(about);

      row("\u23fb", "Quit", "Ctrl+Q", () => invoke("app_quit"), { danger: true });
    },

    extensions: async () => {
      header("Extensions");
      const list = await invoke("ext_list");
      const settings = await invoke("settings_get");
      const hidden = new Set(settings.hidden_extensions || []);
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No extensions loaded. Drop folders or .crx files into crx/ and restart.";
        appMenu.append(empty);
      }
      for (const ext of list) {
        const item = document.createElement("div");
        item.className = "menu-entry entry-row";
        const text = document.createElement("div");
        text.className = "entry-text";
        const name = document.createElement("div");
        name.className = "entry-name";
        name.textContent = ext.version ? ext.name + " " + ext.version : ext.name;
        const state = document.createElement("div");
        state.className = "entry-note" + (ext.status === "error" ? " bad" : "");
        state.textContent = ext.status + (ext.reason ? " \u00b7 " + ext.reason : "");
        text.append(name, state);
        item.append(text);

        if (ext.status === "loaded") {
          const shown = !hidden.has(ext.name);
          const toggle = document.createElement("button");
          toggle.className = "entry-toggle" + (shown ? " on" : "");
          toggle.title = shown ? "Hide from the toolbar" : "Show in the toolbar";
          toggle.textContent = shown ? "\u25c9" : "\u25cb";
          toggle.addEventListener("click", async (event) => {
            event.stopPropagation();
            const next = new Set(hidden);
            if (next.has(ext.name)) next.delete(ext.name);
            else next.add(ext.name);
            await invoke("settings_set", { patch: { hidden_extensions: [...next] } });
            render("extensions");
          });
          item.append(toggle);
        }
        appMenu.append(item);
      }
      divider();
      row("\u25a4", "Open extensions folder", "", act(() => invoke("open_dir", { which: "crx" })));
    },

    scripts: async () => {
      header("Userscripts");
      const list = await invoke("script_list");
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No userscripts loaded. Drop *.user.js files into javascript/ and restart.";
        appMenu.append(empty);
      }
      for (const script of list) {
        const item = document.createElement("div");
        item.className = "menu-entry";
        const name = document.createElement("div");
        name.className = "entry-name";
        name.textContent = script.name;
        const state = document.createElement("div");
        state.className = "entry-note";
        state.textContent = script.run_at + " \u00b7 " + script.matches.join(", ");
        item.append(name, state);
        appMenu.append(item);
      }
      divider();
      row("\u2699", "Open userscripts folder", "", act(() => invoke("open_dir", { which: "scripts" })));
    },

    bookmarks: async () => {
      header("Bookmarks");
      const settings = await invoke("settings_get");
      row(
        settings.show_bookmarks ? "\u2611" : "\u2610",
        "Show bookmarks bar",
        "",
        async () => {
          await invoke("settings_set", { patch: { show_bookmarks: !settings.show_bookmarks } });
          render("bookmarks");
        },
      );
      divider();
      const list = await invoke("bookmark_list");
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No bookmarks yet. Press Ctrl+D on a page, or use Bookmark this page.";
        appMenu.append(empty);
      }
      for (const bookmark of list) {
        const item = document.createElement("button");
        item.className = "row";
        const icon = document.createElement("span");
        icon.className = "ico";
        icon.textContent = "\u2606";
        const label = document.createElement("span");
        label.className = "label";
        label.textContent = bookmark.title || bookmark.url;
        label.title = bookmark.url;
        const remove = document.createElement("span");
        remove.className = "hint";
        remove.textContent = "\u00d7";
        remove.title = "Remove";
        remove.addEventListener("click", async (event) => {
          event.stopPropagation();
          await invoke("bookmark_remove", { url: bookmark.url });
          render("bookmarks");
        });
        item.append(icon, label, remove);
        item.addEventListener("click", async () => {
          await invoke("tab_new", { url: bookmark.url });
          await close();
        });
        appMenu.append(item);
      }
    },

    history: async () => {
      header("History");
      const list = await invoke("history_list", { limit: 60 });
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "Nothing recorded yet.";
        appMenu.append(empty);
      }
      for (const visit of list) {
        const item = document.createElement("button");
        item.className = "row";
        const icon = document.createElement("span");
        icon.className = "ico";
        icon.textContent = "\u21ba";
        const label = document.createElement("span");
        label.className = "label";
        label.textContent = visit.title || visit.url;
        label.title = visit.url;
        const when = document.createElement("span");
        when.className = "hint";
        when.textContent = new Date(visit.at * 1000).toLocaleDateString();
        item.append(icon, label, when);
        item.addEventListener("click", async () => {
          await invoke("tab_new", { url: visit.url });
          await close();
        });
        appMenu.append(item);
      }
      if (list.length) {
        divider();
        row("\u2717", "Clear history", "", async () => {
          await invoke("history_clear");
          render("history");
        });
      }
    },

    tools: async () => {
      header("More tools");
      row("\u2318", "Developer tools", "F12", act(() => invoke("shortcut", { action: "devtools" })));
      row("\u26a0", "Open log file", "", act(() => invoke("open_dir", { which: "logs" })));
      row("\u2261", "Profile folder", "", act(() => invoke("open_dir", { which: "userdata" })));
      row("\u2691", "Export diagnostics", "", async () => {
        const path = await invoke("diag_export");
        report("diagnostics saved to " + path);
        await invoke("open_dir", { which: "diag" });
        await close();
      });
    },

    toolbar: async () => {
      header("Toolbar");
      const settings = await invoke("settings_get");
      const enabled = new Set(settings.toolbar || []);
      const names = {
        downloads: "Downloads",
        translate: "Translate",
        find: "Find in page",
        bookmark: "Bookmark",
        extensions: "Extensions",
      };

      for (const id of Object.keys(names)) {
        const on = enabled.has(id);
        row(on ? "\u2611" : "\u2610", names[id], "", async () => {
          const next = new Set(settings.toolbar || []);
          if (next.has(id)) next.delete(id);
          else next.add(id);
          await invoke("settings_set", { patch: { toolbar: [...next] } });
          render("toolbar");
        });
      }

      divider();
      const loaded = (await invoke("ext_list")).filter((ext) => ext.status === "loaded");
      if (!loaded.length) {
        const note = document.createElement("div");
        note.className = "menu-note";
        note.textContent = "No extensions loaded.";
        appMenu.append(note);
        return;
      }
      const hidden = new Set(settings.hidden_extensions || []);
      for (const ext of loaded) {
        const shown = !hidden.has(ext.name);
        row(shown ? "\u2611" : "\u2610", ext.name, "", async () => {
          const next = new Set(settings.hidden_extensions || []);
          if (next.has(ext.name)) next.delete(ext.name);
          else next.add(ext.name);
          await invoke("settings_set", { patch: { hidden_extensions: [...next] } });
          render("toolbar");
        });
      }
    },

    settings: async () => {
      header("Settings");
      row("\u25d0", "Appearance", "", () =>
        invoke("overlay_open", { kind: "settings&section=appearance" }), { page: true });
      row("\u25b6", "Startup", "", () =>
        invoke("overlay_open", { kind: "settings&section=startup" }), { page: true });
      row("\u25ab", "Toolbar buttons", "", () => render("toolbar"), { page: true });
      row("\u2398", "Theme file", "", () =>
        invoke("overlay_open", { kind: "settings&section=theme" }), { page: true });
      divider();
      row("\u25a4", "Extensions", "", () => render("extensions"), { page: true });
      row("\u2699", "Userscripts", "", () => render("scripts"), { page: true });
      divider();
      row("\u2261", "Open profile folder", "", act(() => invoke("open_dir", { which: "userdata" })));
    },

    help: async () => {
      header("Help");
      row("\u2318", "Keyboard shortcuts", "", () => render("shortcuts"), { page: true });
      row("\u25a4", "Open program folder", "", act(() => invoke("open_dir", { which: "root" })));
      divider();
      const about = document.createElement("div");
      about.className = "menu-note";
      about.textContent =
        "Mix " + info.version + " (" + info.git + ")\n" +
        info.os + " \u00b7 WebView2 " + info.webview;
      appMenu.append(about);
    },

    shortcuts: async () => {
      header("Keyboard shortcuts");
      const keys = [
        ["Ctrl+T", "New tab"],
        ["Ctrl+N", "New window"],
        ["Ctrl+Shift+N", "New incognito tab"],
        ["Ctrl+Shift+T", "Reopen closed tab"],
        ["Ctrl+W", "Close tab"],
        ["Ctrl+Tab", "Next tab"],
        ["Ctrl+Shift+Tab", "Previous tab"],
        ["Ctrl+L", "Focus address bar"],
        ["Ctrl+R / F5", "Reload"],
        ["Ctrl+P", "Print"],
        ["Ctrl+J", "Open downloads"],
        ["Ctrl+D", "Bookmark this page"],
        ["Middle click", "Close tab"],
        ["Ctrl+\u00b1", "Zoom in / out"],
        ["Ctrl+0", "Reset zoom"],
        ["F11", "Full screen"],
        ["F12", "Developer tools (debug)"],
        ["Alt+Shift+\u2190\u2192\u2191\u2193", "Dock the window"],
        ["Ctrl+Q", "Quit"],
      ];
      for (const [combo, what] of keys) {
        const item = document.createElement("div");
        item.className = "row shortcut-row";
        const key = document.createElement("span");
        key.className = "label";
        key.textContent = combo;
        const text = document.createElement("span");
        text.className = "hint";
        text.textContent = what;
        item.append(key, text);
        appMenu.append(item);
      }
    },
  };

  async function render(page) {
    clear();
    await pages[page]();
    report("menu page=" + page + " nodes=" + appMenu.children.length);
  }

  appMenu.addEventListener("pointerdown", (event) => {
    if (event.target === appMenu) invoke("overlay_close");
  });

  await render(params.get("page") || "root");
}
async function bootSuggest() {
  // suggest closes on blur
  window.addEventListener("blur", () => invoke("overlay_close").catch(() => {}));
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") invoke("overlay_close").catch(() => {});
  });
  document.addEventListener("mousedown", (event) => {
    if (!event.target.closest(".suggest") && !event.target.closest("button")) {
      invoke("overlay_close").catch(() => {});
    }
  });


  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  window.__mixSuggest = (items) => {
    while (menu.firstChild) menu.removeChild(menu.firstChild);
    if (!items.length) {
      const note = document.createElement("div");
      note.className = "menu-note";
      note.textContent = "No matches";
      menu.append(note);
      return;
    }
    for (const item of items) {
      const row = document.createElement("button");
      row.className = "row";
      const icon = document.createElement("span");
      icon.className = "ico";
      icon.textContent = item.kind === "bookmark" ? "\u2606" : "\u21ba";
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = item.title || item.url;
      const host = document.createElement("span");
      host.className = "hint";
      try {
        host.textContent = new URL(item.url).host;
      } catch {
        host.textContent = "";
      }
      row.append(icon, label, host);
      row.addEventListener("click", async () => {
        await invoke("tab_navigate", { url: item.url });
        await invoke("overlay_close");
      });
      menu.append(row);
    }
  };

  await invoke("suggest_refresh").catch(() => {});
}

async function bootZoom() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const bar = document.createElement("div");
  bar.className = "zoom-bar";

  const minus = document.createElement("button");
  minus.className = "find-step";
  minus.textContent = "\u2212";
  const level = document.createElement("button");
  level.className = "zoom-level";
  level.title = "Reset to 100%";
  const plus = document.createElement("button");
  plus.className = "find-step";
  plus.textContent = "+";

  let zoom = await invoke("zoom_get");
  const paint = (value) => {
    zoom = value;
    level.textContent = Math.round(value * 100) + "%";
  };
  paint(zoom);

  minus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: -0.1 })));
  plus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 0.1 })));
  level.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 1 - zoom })));

  bar.append(minus, level, plus);
  menu.append(bar);
}

async function bootFind() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const bar = document.createElement("div");
  bar.className = "find-bar";
  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = "Find in page";
  input.className = "find-input";
  const prev = document.createElement("button");
  prev.className = "find-step";
  prev.textContent = "\u2191";
  const next = document.createElement("button");
  next.className = "find-step";
  next.textContent = "\u2193";
  bar.append(input, prev, next);
  menu.append(bar);

  const search = async (forward, restart) => {
    if (!input.value) return;
    await invoke("find_in_page", { text: input.value, forward, restart }).catch(() => {});
  };

  input.addEventListener("input", () => search(true, true));
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      search(!event.shiftKey, false);
    }
  });
  next.addEventListener("click", () => search(true, false));
  prev.addEventListener("click", () => search(false, false));

  setTimeout(() => input.focus(), 30);
}

async function bootExtInfo() {
  appMenu.hidden = false;
  themePanel.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const wanted = params.get("name") || "";
  const list = await invoke("ext_list");
  const ext = list.find((item) => item.name === wanted) || list[0];
  if (!ext) return;

  const head = document.createElement("div");
  head.className = "menu-header";
  if (ext.icon) {
    const img = document.createElement("img");
    img.src = ext.icon;
    img.alt = "";
    img.className = "ext-badge";
    head.append(img);
  }
  const title = document.createElement("span");
  title.textContent = ext.version ? `${ext.name} ${ext.version}` : ext.name;
  head.append(title);
  appMenu.append(head);

  const note = (text) => {
    if (!text) return;
    const line = document.createElement("div");
    line.className = "menu-note";
    line.textContent = text;
    appMenu.append(line);
  };
  note(ext.description || "");
  note(ext.popup ? `Popup: ${ext.popup}` : "No toolbar popup");
  if (ext.permissions && ext.permissions.length) {
    note(`Permissions: ${ext.permissions.slice(0, 8).join(", ")}`);
  }

  const divider = () =>
    appMenu.append(Object.assign(document.createElement("div"), { className: "divider" }));
  divider();

  const action = (glyph, label, run) => {
    const button = document.createElement("button");
    button.className = "row";
    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label;
    button.append(icon, text, document.createElement("span"));
    button.addEventListener("click", run);
    appMenu.append(button);
  };

  action("\u25a4", "Open extension folder", async () => {
    await invoke("open_dir", { which: "extensions" });
    await close();
  });
  action("\u21ba", "Reload extensions", async () => {
    report("Extensions reload on restart");
    await close();
  });
}

async function bootChromeMenu(kind) {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const item = (label, run) => {
    const button = document.createElement("button");
    button.className = "row plain";
    button.textContent = label;
    button.addEventListener("click", async () => {
      await run();
      await invoke("overlay_close").catch(() => {});
    });
    menu.append(button);
  };

  if (kind === "url") {
    item("Cut", () => invoke("url_action", { action: "cut" }));
    item("Copy", () => invoke("url_action", { action: "copy" }));
    item("Paste", () => invoke("url_action", { action: "paste" }));
    const divider = document.createElement("div");
    divider.className = "divider";
    menu.append(divider);
    item("Select all", () => invoke("url_action", { action: "select-all" }));
    return;
  }

  item("New tab", () => invoke("tab_new", { url: null }));
  item("Reload", () => invoke("tab_reload"));
  const divider = document.createElement("div");
  divider.className = "divider";
  menu.append(divider);
  item("Settings", () => invoke("overlay_open", { kind: "app_menu&page=settings" }));
}

async function bootMenu() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const context = await invoke("menu_context_get");

  const divider = () => menu.append(Object.assign(document.createElement("div"), { className: "divider" }));

  const item = (glyph, label, hint, run) => {
    const button = document.createElement("button");
    button.className = "row";
    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label;
    const key = document.createElement("span");
    key.className = "hint";
    key.textContent = hint || "";
    button.append(icon, text, key);
    button.addEventListener("click", run);
    menu.append(button);
    return button;
  };

  const copy = async (value) => {
    try {
      await navigator.clipboard.writeText(value || "");
    } catch (error) {
      report("clipboard failed: " + error);
    }
  };

  // Only offer what applies to what was clicked.
  if (context.link) {
    item("\u2197", "Open link in new tab", "", () => invoke("tab_new", { url: context.link }));
    item("\u2398", "Copy link address", "", () => copy(context.link));
    divider();
  }
  if (context.image) {
    item("\u25a3", "Open image in new tab", "", () => invoke("tab_new", { url: context.image }));
    item("\u2398", "Copy image address", "", () => copy(context.image));
    item("\u2b07", "Save image as...", "", async () => {
      const name = (context.image.split("/").pop() || "image").split("?")[0];
      try {
        await invoke("download_url", { url: context.image, fileName: name });
      } catch (error) {
        report("save image failed: " + error);
      }
      await invoke("overlay_close");
    });
    divider();
  }
  if (context.selection) {
    item("\u2398", "Copy", "", () => copy(context.selection));
    divider();
  }
  if (context.editable) {
    item("\u2702", "Cut", "", () => document.execCommand("cut"));
    item("\u2398", "Copy", "", () => document.execCommand("copy"));
    item("\u2399", "Paste", "", () => document.execCommand("paste"));
    divider();
  }

  item("\u2190", "Back", "", () => invoke("tab_nav", { action: "back" }));
  item("\u2192", "Forward", "", () => invoke("tab_nav", { action: "forward" }));
  item("\u21bb", "Reload", "Ctrl+R", () => invoke("tab_nav", { action: "reload" }));
  divider();
  item("\u2399", "Print", "Ctrl+P", () => invoke("tab_print"));
  if (context.selection) {
    item("\u4e16", "Translate selection", "", async () => {
      await invoke("translate_page", { target: context.selection }).catch((error) => report(String(error)));
      await invoke("overlay_close");
    });
  }
  item("\u4e16", "Translate page", "", async () => {
    await invoke("translate_page", { target: null }).catch((error) => report(String(error)));
    await invoke("overlay_close");
  });
  item("\u2691", "Inspect", "", () => invoke("shortcut", { action: "devtools" }));

  // Dismiss when the pointer lands anywhere in the panel but not on a row.
  menu.addEventListener("pointerdown", (event) => {
    if (event.target === menu) invoke("overlay_close");
  });
}

async function bootPermission() {
  permissionPanel.hidden = false;
  themePanel.hidden = true;
  appMenu.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const id = Number(params.get("id") || 0);
  const origin = params.get("origin") || "this page";
  const name = params.get("kind") || "something";

  const text = document.getElementById("permission-text");
  text.textContent = `${origin} wants to use ${name}.`;

  const answer = async (allow) => {
    await invoke("permission_reply", { id, allow });
  };
  document.getElementById("permission-allow").addEventListener("click", () => answer(true));
  document.getElementById("permission-block").addEventListener("click", () => answer(false));
}

// Escape closes any overlay, whatever kind it is.
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    invoke("overlay_close").catch(() => {});
  }
});

async function boot() {
  report(`overlay kind=${kind}`);
  if (kind === "app_menu") {
    await bootAppMenu();
    return;
  }
  if (kind === "context_menu") {
    await bootMenu();
    return;
  }
  if (kind === "zoom") {
    await bootZoom();
    return;
  }
  if (kind === "find") {
    await bootFind();
    return;
  }
  if (kind === "url_menu") {
    await bootChromeMenu("url");
    return;
  }
  if (kind === "chrome_menu") {
    await bootChromeMenu("chrome");
    return;
  }
  if (kind === "ext_info") {
    await bootExtInfo();
    return;
  }
  if (kind === "suggest") {
    await bootSuggest();
    return;
  }
  if (kind === "permission") {
    await bootPermission();
    return;
  }

  const section = params.get("section") || "appearance";
  const titles = { appearance: "Appearance", startup: "Startup", theme: "Theme file" };
  document.getElementById("panel-title").textContent = titles[section] || "Settings";
  for (const block of document.querySelectorAll("[data-section]")) {
    block.hidden = block.dataset.section !== section;
  }
  document.getElementById("presets").hidden = section !== "appearance";
  document.getElementById("panel-back").addEventListener("click", () =>
    invoke("overlay_open", { kind: "app_menu&page=settings" }),
  );

  current = await invoke("theme_get");
  render(current);

  autostartBox.checked = await invoke("autostart_get");

  const settings = await invoke("settings_get");
  restoreBox.checked = !!settings.restore_session;

  for (const input of Object.values(fields)) input.addEventListener("input", preview);
  radius.addEventListener("input", preview);
  tabRadius.addEventListener("input", preview);
  fontSize.addEventListener("input", preview);
  fontFamily.addEventListener("change", preview);

  for (const button of document.querySelectorAll("[data-preset]")) {
    button.addEventListener("click", async () => {
      const preset = PRESETS[button.dataset.preset];
      current = await invoke("theme_apply", {
        theme: { ...current, colors: { ...current.colors, ...preset }, radius: preset.radius },
        persist: false,
      });
      render(current);
    });
  }

  const themeError = document.getElementById("theme-error");

  document.getElementById("theme-export").addEventListener("click", async () => {
    try {
      const path = await invoke("theme_export");
      themeError.textContent = "";
      report("theme exported to " + path);
    } catch (error) {
      themeError.textContent = String(error);
    }
  });

  document.getElementById("theme-apply-json").addEventListener("click", async () => {
    const box = document.getElementById("theme-json");
    try {
      current = await invoke("theme_import", { json: box.value });
      render(current);
      box.value = "";
      themeError.textContent = "";
    } catch (error) {
      themeError.textContent = String(error);
    }
  });

  document.getElementById("save").addEventListener("click", async () => {
    await invoke("theme_apply", { theme: current, persist: true });
    await invoke("settings_set", { patch: { restore_session: restoreBox.checked } });
    await invoke("autostart_set", { enabled: autostartBox.checked });
    await close();
  });

  document.getElementById("cancel").addEventListener("click", async () => {
    await invoke("theme_revert");
    await close();
  });

  document.addEventListener("keydown", async (event) => {
    if (event.key === "Escape") {
      await invoke("theme_revert");
      await close();
    }
  });
}

boot().catch((error) => {
  console.error("overlay boot failed:", error);
  report(`overlay boot failed: ${error && error.message ? error.message : error}`);
});