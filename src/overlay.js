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
  const act = (fn) => async () => {
    await fn();
    await close();
  };

  const divider = () => appMenu.append(Object.assign(document.createElement("div"), { className: "divider" }));

  const row = (glyph, label, hint, run, options = {}) => {
    const button = document.createElement("button");
    button.className = "row" + (options.danger ? " danger" : "");

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
    if (run) button.addEventListener("click", run);
    appMenu.append(button);
    return button;
  };

  row("+", "New tab", "Ctrl+T", act(() => invoke("tab_new", {})));
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

  const zoomControls = document.createElement("div");
  zoomControls.className = "zoom-controls";
  zoomControls.append(minus, level, plus);
  zoomRow.append(zoomIcon, zoomControls);
  appMenu.append(zoomRow);

  row("\u25a3", "Full screen", "F11", act(() => invoke("window_fullscreen", { enabled: null })));
  divider();

  row("\u2399", "Print", "Ctrl+P", act(() => invoke("tab_print")));
  row("\u2717", "Clear browsing data", "", async () => {
    await invoke("app_clear_data");
    await close();
  });
  divider();

  row("\u25a4", "Extensions", "", act(() => invoke("open_dir", { which: "crx" })));
  row("\u2699", "Userscripts", "", act(() => invoke("open_dir", { which: "scripts" })));
  row("\u2261", "Profile folder", "", act(() => invoke("open_dir", { which: "userdata" })));
  row("\u26a0", "Logs", "", act(() => invoke("open_dir", { which: "logs" })));
  row("\u2691", "Export diagnostics", "", async () => {
    const path = await invoke("diag_export");
    report("diagnostics saved to " + path);
    await invoke("open_dir", { which: "diag" });
    await close();
  });
  divider();

  row("\u2699", "Preferences", "", () => invoke("overlay_open", { kind: "settings" }));

  const info = await invoke("app_info");
  const about = document.createElement("div");
  about.className = "about";
  about.textContent = "Mix " + info.version + " (" + info.git + ")";
  appMenu.append(about);

  row("\u23fb", "Quit", "Ctrl+Q", () => invoke("app_quit"), { danger: true });

  const box = appMenu.getBoundingClientRect();
  const style = getComputedStyle(appMenu);
  report(
    "menu rows=" + appMenu.querySelectorAll(".row").length +
    " box=" + Math.round(box.width) + "x" + Math.round(box.height) +
    " at " + Math.round(box.left) + "," + Math.round(box.top) +
    " bg=" + style.backgroundColor +
    " color=" + style.color +
    " display=" + style.display
  );
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