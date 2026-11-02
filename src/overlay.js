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

async function boot() {
  current = await invoke("theme_get");
  render(current);

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
