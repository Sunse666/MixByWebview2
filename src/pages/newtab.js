import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const greeting = document.getElementById("greeting");
const input = document.getElementById("q");
const tiles = document.getElementById("tiles");

function hostOf(url) {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch (error) {
    return url;
  }
}

function open(target) {
  const value = target.trim();
  if (!value) return;
  invoke("tab_navigate", { url: value }).catch(() => {});
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");

  const hour = new Date().getHours();
  greeting.textContent =
    hour < 6 ? t("Good night") : hour < 12 ? t("Good morning") : hour < 18 ? t("Good afternoon") : t("Good evening");
  input.placeholder = t("Search or enter address");
  document.getElementById("go").textContent = t("Go");

  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") open(input.value);
  });
  document.getElementById("go").addEventListener("click", () => open(input.value));
  input.focus();

  // Most visited hosts become the shortcut tiles.
  const visits = (await invoke("history_list", { limit: 400 }).catch(() => [])) || [];
  const counts = new Map();
  for (const visit of visits) {
    const host = hostOf(visit.url);
    if (!host || host.startsWith("tauri.localhost")) continue;
    const entry = counts.get(host) || { host, url: visit.url, count: 0 };
    entry.count += 1;
    counts.set(host, entry);
  }

  const top = [...counts.values()].sort((a, b) => b.count - a.count).slice(0, 12);
  tiles.replaceChildren(
    ...top.map((entry) => {
      const tile = document.createElement("div");
      tile.className = "tile";
      const dot = document.createElement("div");
      dot.className = "dot";
      dot.textContent = (entry.host[0] || "?").toUpperCase();
      const name = document.createElement("div");
      name.className = "name";
      name.textContent = entry.host;
      tile.append(dot, name);
      tile.title = entry.host;
      tile.addEventListener("click", () => invoke("tab_navigate", { url: entry.url }).catch(() => {}));
      return tile;
    }),
  );
}

boot();
