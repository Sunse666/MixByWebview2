import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";
import { addressFor } from "../address.js";

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

/**
 * Sends the reader where the box asked to go.
 *
 * Through `addressFor`, the same decision the chrome strip's address bar makes: a typed
 * address navigates, anything else is a search. Handing the raw text to `tab_navigate` is
 * what used to happen, and it rejected `rust async` as "not a web address" - replacing the
 * new tab page with the error page for a perfectly ordinary thing to type.
 */
function open(target, settings) {
  const value = target.trim();
  if (!value) return;
  invoke("tab_navigate", { url: addressFor(value, settings) }).catch(() => {});
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
    if (event.key === "Enter") open(input.value, settings);
  });
  document.getElementById("go").addEventListener("click", () => open(input.value, settings));
  input.focus();

  const visits = (await invoke("history_list", { limit: 400 }).catch(() => [])) || [];
  const counts = new Map();
  for (const visit of visits) {
    const host = hostOf(visit.url);
    if (!host || host.startsWith("tauri.localhost")) continue;
    const entry = counts.get(host) || { host, url: visit.url, count: 0 };
    // The count the backend keeps, not one per row. A row is one url, so adding 1 per row
    // ranked a site by how many of its pages had each been opened once - which is not how
    // often it is used. Opening a page again does not add a row, it adds to this number.
    entry.count += visit.visits || 1;
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