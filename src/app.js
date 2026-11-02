const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const urlInput = document.getElementById("url");
const tabStrip = document.getElementById("tabs");

let tabs = [];
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
    downloadPill.textContent = "v " + latest.file_name;
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
}

// Rust calls this after every theme change, including live preview.
window.__mixApplyTheme = applyTheme;

function renderTabs() {
  const nodes = tabs.map((tab) => {
    const el = document.createElement("div");
    const classes = ["tab"];
    if (tab.id === selected) classes.push("active");
    if (!tab.alive) classes.push("suspended");
    el.className = classes.join(" ");
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
  await listen("download:changed", (event) => renderDownloads(event.payload));
  await listen("ui:new-tab", () => invoke("tab_new", {}));
  await listen("ui:focus-url", () => {
    urlInput.focus();
    urlInput.select();
  });
  renderDownloads(await invoke("download_list"));

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
    let action = null;
    if (event.key === "F5") action = "reload";
    else if (event.key === "F12") action = "devtools";
    else if (event.ctrlKey || event.metaKey) {
      switch (event.key) {
        case "t": action = "new_tab"; break;
        case "w": action = "close_tab"; break;
        case "l": action = "focus_url"; break;
        case "r": action = "reload"; break;
        case "Tab": action = event.shiftKey ? "prev_tab" : "next_tab"; break;
        case "=":
        case "+": action = "zoom_in"; break;
        case "-": action = "zoom_out"; break;
        case "0": action = "zoom_reset"; break;
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

  downloadPill.addEventListener("click", () => {
    const id = Number(downloadPill.dataset.id);
    if (!Number.isNaN(id)) invoke("download_open_dir", { id });
  });

  document.getElementById("theme-btn").addEventListener("click", () => invoke("overlay_open", { kind: "settings" }));
  document.getElementById("tab-new").addEventListener("click", () => invoke("tab_new", {}));

  urlInput.focus();
}

boot().catch((error) => console.error("boot failed:", error));