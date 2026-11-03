const { invoke } = window.__TAURI__.core;

const root = document.documentElement;
const fields = {
  bg: document.getElementById("bg"),
  chrome: document.getElementById("chrome"),
  accent: document.getElementById("accent"),
  text: document.getElementById("text"),
};
const radius = document.getElementById("radius");
const radiusOut = document.getElementById("radius-out");
const fontSize = document.getElementById("font-size");
const fontOut = document.getElementById("font-out");

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
}

// Rust pushes every change here too, so preview and chrome stay in sync.
window.__mixApplyTheme = applyTheme;

function render(theme) {
  applyTheme(theme);
  for (const [key, input] of Object.entries(fields)) input.value = theme.colors[key];
  radius.value = String(Math.round(theme.radius));
  radiusOut.textContent = `${Math.round(theme.radius)}px`;
  fontSize.value = String(Math.round(theme.font_size));
  fontOut.textContent = `${Math.round(theme.font_size)}px`;
}

async function preview() {
  const next = {
    ...current,
    colors: {
      ...current.colors,
      bg: fields.bg.value,
      chrome: fields.chrome.value,
      accent: fields.accent.value,
      text: fields.text.value,
    },
    radius: Number(radius.value),
    font_size: Number(fontSize.value),
  };
  current = await invoke("theme_apply", { theme: next, persist: false });
  radiusOut.textContent = `${Math.round(current.radius)}px`;
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
      row("\u25d1", "New incognito tab", "Ctrl+Shift+N", act(() => invoke("tab_new", { incognito: true })));
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
      row("\u2717", "Clear browsing data", "Ctrl+Shift+Del", async () => {
        await invoke("app_clear_data");
        await close();
      });
      divider();
      row("\u25a4", "Extensions", "", () => render("extensions"), { page: true });
      row("\u2699", "Userscripts", "", () => render("scripts"), { page: true });
      row("\u2261", "More tools", "", () => render("tools"), { page: true });
      divider();
      row("\u2699", "Preferences", "", () => invoke("overlay_open", { kind: "settings" }));
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
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No extensions loaded. Drop folders or .crx files into crx/ and restart.";
        appMenu.append(empty);
      }
      for (const ext of list) {
        const item = document.createElement("div");
        item.className = "menu-entry";
        const name = document.createElement("div");
        name.className = "entry-name";
        name.textContent = ext.version ? ext.name + " " + ext.version : ext.name;
        const state = document.createElement("div");
        state.className = "entry-note" + (ext.status === "error" ? " bad" : "");
        state.textContent = ext.status + (ext.reason ? " \u00b7 " + ext.reason : "");
        item.append(name, state);
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
        ["Ctrl+Shift+N", "New incognito tab"],
        ["Ctrl+W", "Close tab"],
        ["Ctrl+Tab", "Next tab"],
        ["Ctrl+Shift+Tab", "Previous tab"],
        ["Ctrl+L", "Focus address bar"],
        ["Ctrl+R / F5", "Reload"],
        ["Ctrl+P", "Print"],
        ["Ctrl+J", "Open downloads"],
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

  await render(params.get("page") || "root");
}
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
  if (kind === "permission") {
    await bootPermission();
    return;
  }

  current = await invoke("theme_get");
  render(current);

  autostartBox.checked = await invoke("autostart_get");

  const extensionList = document.getElementById("extensions");
  const loaded = await invoke("ext_list");
  if (!loaded.length) {
    const empty = document.createElement("li");
    empty.textContent = "none loaded";
    extensionList.append(empty);
  }
  const scriptList = document.getElementById("scripts");
  const scripts = await invoke("script_list");
  if (!scripts.length) {
    const empty = document.createElement("li");
    empty.textContent = "none loaded";
    scriptList.append(empty);
  }
  for (const script of scripts) {
    const item = document.createElement("li");
    const name = document.createElement("span");
    name.className = "ext-name";
    name.textContent = script.name;
    name.title = script.matches.join(", ");
    const state = document.createElement("span");
    state.className = "ext-state";
    state.textContent = script.run_at;
    item.append(name, state);
    scriptList.append(item);
  }

  for (const extension of loaded) {
    const item = document.createElement("li");
    const name = document.createElement("span");
    name.className = "ext-name";
    name.textContent = extension.version
      ? `${extension.name} v${extension.version}`
      : extension.name;
    name.title = extension.reason || extension.dir;
    const state = document.createElement("span");
    state.className = extension.status === "error" ? "ext-state error" : "ext-state";
    state.textContent = extension.status;
    item.append(name, state);
    extensionList.append(item);
  }

  const settings = await invoke("settings_get");
  restoreBox.checked = !!settings.restore_session;

  for (const input of Object.values(fields)) input.addEventListener("input", preview);
  radius.addEventListener("input", preview);
  fontSize.addEventListener("input", preview);

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