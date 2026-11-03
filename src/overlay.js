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

async function bootAppMenu() {
  appMenu.hidden = false;
  themePanel.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const item = (label, run, hint) => {
    const button = document.createElement("button");
    button.textContent = label;
    if (hint) button.title = hint;
    button.addEventListener("click", run);
    appMenu.append(button);
  };

  const info = await invoke("app_info");
  const version = document.createElement("div");
  version.className = "app-version";
  version.textContent = `Mix v${info.version} (${info.git})`;
  appMenu.append(version);

  const act = (fn) => async () => {
    await fn();
    await invoke("overlay_close");
  };

  item("New tab", act(() => invoke("tab_new", {})));
  item("Downloads", act(() => invoke("open_dir", { which: "downloads" })));
  item("Extensions folder", act(() => invoke("open_dir", { which: "crx" })));
  item("Userscripts folder", act(() => invoke("open_dir", { which: "scripts" })));
  item("Data folder", act(() => invoke("open_dir", { which: "userdata" })));
  item("Logs", act(() => invoke("open_dir", { which: "logs" })));
  appMenu.append(document.createElement("hr"));
  item("Preferences...", () => invoke("overlay_open", { kind: "settings" }));
  item("Quit", () => invoke("app_quit"));
}
const themePanel = document.getElementById("theme-panel");

async function closeSelf() {
  await invoke("overlay_close");
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

function addItem(label, run) {
  const button = document.createElement("button");
  button.textContent = label;
  button.addEventListener("click", run);
  menu.append(button);
  return button;
}

function addSeparator() {
  menu.append(document.createElement("hr"));
}

async function bootMenu() {
  menu.hidden = false;
  themePanel.hidden = true;

  applyTheme(await invoke("theme_get"));
  const context = await invoke("menu_context_get");
  const openInTab = async (url) => {
    await invoke("tab_new", { url });
    await closeSelf();
  };

  if (context.link) {
    addItem("Open link in new tab", () => openInTab(context.link));
    addItem("Copy link address", async () => {
      await copyText(context.link);
      await closeSelf();
    });
  }
  if (context.image) {
    addItem("Open image in new tab", () => openInTab(context.image));
    addItem("Copy image address", async () => {
      await copyText(context.image);
      await closeSelf();
    });
  }
  if (context.selection) {
    addItem("Copy", async () => {
      await copyText(context.selection);
      await closeSelf();
    });
    addItem("Search for the selection", () =>
      openInTab("https://www.bing.com/search?q=" + encodeURIComponent(context.selection)),
    );
  }

  if (context.link || context.image || context.selection) addSeparator();

  addItem("Back", async () => {
    await invoke("tab_nav", { action: "back" });
    await closeSelf();
  });
  addItem("Forward", async () => {
    await invoke("tab_nav", { action: "forward" });
    await closeSelf();
  });
  addItem("Reload", async () => {
    await invoke("tab_nav", { action: "reload" });
    await closeSelf();
  });
  addSeparator();
  addItem("Close menu", closeSelf);

  document.addEventListener("keydown", async (event) => {
    if (event.key === "Escape") await closeSelf();
  });
}

const permissionPanel = document.getElementById("permission-panel");

async function bootPermission() {
  permissionPanel.hidden = false;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const origin = params.get("origin") || "this site";
  const action = params.get("kind") || "permission";
  const id = Number(params.get("id") || "0");

  document.getElementById("permission-text").textContent = `${origin} wants to use ${action}`;

  const send = async (allow) => {
    await invoke("permission_reply", { id, allow });
  };

  document.getElementById("permission-allow").addEventListener("click", () => send(true));
  document.getElementById("permission-block").addEventListener("click", () => send(false));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") send(false);
  });
}

const autostartBox = document.getElementById("autostart");
const restoreBox = document.getElementById("restore");

async function boot() {
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

boot().catch((error) => console.error("overlay boot failed:", error));