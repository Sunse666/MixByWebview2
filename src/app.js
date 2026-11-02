const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const urlInput = document.getElementById("url");
const tabTitle = document.getElementById("tab-title");

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

// The Rust side calls this after every theme change, including live preview.
window.__mixApplyTheme = applyTheme;

let currentTab = null;

async function boot() {
  applyTheme(await invoke("theme_get"));
  const layout = await invoke("layout_get");
  currentTab = layout.selected;

  await listen("tab:title", (event) => {
    tabTitle.textContent = event.payload.title || "New tab";
    document.title = event.payload.title || "Mix";
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
    if (event.target.closest("button")) return;
    invoke("window_drag");
  });
  drag.addEventListener("dblclick", (event) => {
    if (event.target.closest("button")) return;
    invoke("window_control", { action: "max" });
  });

  urlInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      invoke("tab_navigate", { url: urlInput.value });
      urlInput.blur();
    }
    if (event.key === "Escape") urlInput.blur();
  });

  document.getElementById("theme-btn").addEventListener("click", () => invoke("overlay_open", { kind: "settings" }));
  document.getElementById("tab-new").addEventListener("click", () => invoke("tab_new", {}));
  document.getElementById("tab-close").addEventListener("click", () => {
    if (currentTab) invoke("tab_close", { id: currentTab });
  });

  urlInput.focus();
}

boot().catch((error) => console.error("boot failed:", error));
