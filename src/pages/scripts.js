import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

const PAGES = {
  history: "http://tauri.localhost/pages/history.html",
  bookmarks: "http://tauri.localhost/pages/bookmarks.html",
  extensions: "http://tauri.localhost/pages/extensions.html",
  downloads: "http://tauri.localhost/pages/downloads.html",
  settings: "http://tauri.localhost/pages/settings.html",
};

function navItem(glyph, label, active, page) {
  const item = document.createElement("div");
  item.className = "nav-item" + (active ? " active" : "");
  const icon = document.createElement("span");
  icon.textContent = glyph;
  const text = document.createElement("span");
  text.textContent = t(label);
  item.append(icon, text);
  if (page) {
    item.addEventListener("click", () => {
      invoke("tab_navigate", { url: page }).catch(() => invoke("tab_new", { url: page }).catch(() => {}));
    });
  }
  nav.append(item);
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Userscripts");

  navItem("⟲", "History", false, PAGES.history);
  navItem("☆", "Bookmarks", false, PAGES.bookmarks);
  navItem("⧉", "Extensions", false, PAGES.extensions);
  navItem("⇩", "Downloads", false, PAGES.downloads);
  navItem("⚙", "Settings", false, PAGES.settings);

  const head = document.createElement("h2");
  head.textContent = t("Userscripts");
  main.append(head);

  const bar = document.createElement("div");
  bar.className = "toolbar";
  const folder = document.createElement("button");
  folder.className = "button";
  folder.textContent = t("Open scripts folder");
  folder.addEventListener("click", () => invoke("open_dir", { which: "scripts" }).catch(() => {}));
  bar.append(folder);
  main.append(bar);

  const grid = document.createElement("div");
  grid.className = "grid";
  main.append(grid);

  const paint = async () => {
    const list = (await invoke("script_list").catch(() => [])) || [];
    grid.replaceChildren();

    if (!list.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("No userscripts.");
      main.append(empty);
      return;
    }

    for (const script of list) {
      const card = document.createElement("div");
      card.className = "card";

      const title = document.createElement("div");
      title.className = "card-title";
      title.textContent = script.name;

      const runAt = document.createElement("div");
      runAt.className = "card-note";
      runAt.textContent = t(script.run_at) + " · " + (script.matches || []).slice(0, 3).join(", ");

      const actions = document.createElement("div");
      actions.className = "card-actions";

      const toggle = document.createElement("button");
      toggle.className = "switch";
      toggle.setAttribute("role", "switch");
      toggle.setAttribute("aria-checked", String(!script.disabled));
      toggle.title = script.disabled ? t("Disabled") : t("Enabled");
      toggle.addEventListener("click", async () => {
        const enable = toggle.getAttribute("aria-checked") !== "true";
        toggle.setAttribute("aria-checked", String(enable));
        await invoke("script_set_enabled", { path: script.path, enabled: enable }).catch(() => {});
        await paint();
      });

      const remove = document.createElement("button");
      remove.className = "button";
      remove.textContent = t("Remove");
      remove.addEventListener("click", async () => {
        await invoke("script_remove", { path: script.path }).catch(() => {});
        await paint();
      });

      const spacer = document.createElement("div");
      spacer.className = "spacer";

      actions.append(remove, spacer, toggle);
      card.append(title, runAt, actions);
      grid.append(card);
    }
  };

  await paint();
}

boot();
