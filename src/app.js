const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const urlInput = document.getElementById("url");
const tabStrip = document.getElementById("tabs");

let tabs = [];
let selected = null;

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

// Rust calls this after every theme change, including live preview.
window.__mixApplyTheme = applyTheme;

function renderTabs() {
  const nodes = tabs.map((tab) => {
    const el = document.createElement("div");
    el.className = tab.id === selected ? "tab active" : "tab";
    el.title = tab.url;

    const label = document.createElement("span");
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

    el.append(label, close);
    el.addEventListener("mousedown", (event) => {
      if (event.target.closest("button")) return;
      invoke("tab_select", { id: tab.id });
    });
    return el;
  });
  tabStrip.replaceChildren(...nodes);
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
  syncUrl();
}

async function boot() {
  applyTheme(await invoke("theme_get"));
  setTabs(await invoke("tab_list"));

  await listen("tabs:changed", (event) => setTabs(event.payload));
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

  urlInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      invoke("tab_navigate", { url: urlInput.value });
      urlInput.blur();
    }
    if (event.key === "Escape") {
      syncUrl();
      urlInput.blur();
    }
  });

  document.getElementById("theme-btn").addEventListener("click", () => invoke("overlay_open", { kind: "settings" }));
  document.getElementById("tab-new").addEventListener("click", () => invoke("tab_new", {}));

  urlInput.focus();
}

boot().catch((error) => console.error("boot failed:", error));
