import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const invoke = withFeedback(window.__TAURI__.core.invoke);

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
  document.title = t("New tab");

  const hour = new Date().getHours();
  greeting.textContent =
    hour < 6 ? t("Good night") : hour < 12 ? t("Good morning") : hour < 18 ? t("Good afternoon") : t("Good evening");
  if (settings && settings.newtab_show_search === false) {
    input.closest(".search")?.remove();
  }

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

  const icons = (await invoke("favicon_map").catch(() => ({}))) || {};
  const limit = (settings && settings.newtab_tiles) || 12;
  const top = [...counts.values()].sort((a, b) => b.count - a.count).slice(0, limit);

  if (settings && settings.newtab_show_sites === false) {
    tiles.style.display = "none";
    return;
  }
  if (settings && settings.newtab_columns) {
    tiles.style.gridTemplateColumns = "repeat(" + settings.newtab_columns + ", 1fr)";
  }
  if (settings && settings.newtab_radius !== undefined) {
    tiles.style.setProperty("--tile-radius", settings.newtab_radius + "%");
  }
  tiles.replaceChildren(
    ...top.map((entry) => {
      const tile = document.createElement("div");
      tile.className = "tile";
      const dot = document.createElement("div");
      dot.className = "dot";
      const icon = icons[entry.host.replace(/^www\./, "").toLowerCase()];
      if (icon) {
        const img = document.createElement("img");
        img.src = icon;
        img.alt = "";
        img.style.width = "20px";
        img.style.height = "20px";
        img.addEventListener("error", () => {
          dot.replaceChildren(
            document.createTextNode((entry.host.replace(/^www\./, "")[0] || "?").toUpperCase()),
          );
        });
        dot.append(img);
      } else {
        dot.textContent = (entry.host.replace(/^www\./, "")[0] || "?").toUpperCase();
      }
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