import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

const PAGES = {
  history: "http://tauri.localhost/pages/history.html",
  extensions: "http://tauri.localhost/pages/extensions.html",
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

function hostOf(url) {
  try {
    return new URL(url).host || url;
  } catch (error) {
    return url;
  }
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Bookmarks");

  navItem("⟲", "History", false, PAGES.history);
  navItem("☆", "Bookmarks", true, null);
  navItem("⧉", "Extensions", false, PAGES.extensions);
  navItem("⚙", "Settings", false, PAGES.settings);

  const head = document.createElement("h2");
  head.textContent = t("Bookmarks");
  main.append(head);

  const bar = document.createElement("div");
  bar.className = "toolbar";
  const search = document.createElement("input");
  search.placeholder = t("Search bookmarks");
  bar.append(search);
  main.append(bar);

  const list = document.createElement("div");
  main.append(list);

  const icons = (await invoke("favicon_map").catch(() => ({}))) || {};
  const iconFor = (url) => {
    const host = hostOf(url).replace(/^www\./, "").toLowerCase();
    const icon = icons[host];
    return icon || "";
  };

  let items = (await invoke("bookmark_list").catch(() => [])) || [];

  const paint = () => {
    list.replaceChildren();
    const needle = search.value.trim().toLowerCase();
    const rows = items.filter(
      (item) =>
        !needle ||
        item.url.toLowerCase().includes(needle) ||
        (item.title || "").toLowerCase().includes(needle),
    );

    if (!rows.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("Nothing recorded yet.");
      list.append(empty);
      return;
    }

    for (const item of rows) {
      const row = document.createElement("div");
      row.className = "entry";

      const mark = document.createElement("span");
      mark.className = "fav";
      const icon = iconFor(item.url);
      if (icon) {
        const img = document.createElement("img");
        img.src = icon;
        img.alt = "";
        img.addEventListener("error", () => {
          mark.replaceChildren(document.createTextNode((hostOf(item.url).replace(/^www\./, "")[0] || "?").toUpperCase()));
          mark.classList.add("glyph");
        });
        mark.append(img);
      } else {
        mark.textContent = (hostOf(item.url).replace(/^www\./, "")[0] || "?").toUpperCase();
        mark.classList.add("glyph");
      }

      const title = document.createElement("span");
      title.className = "title";
      title.textContent = item.title || item.url;

      const host = document.createElement("span");
      host.className = "host";
      host.textContent = hostOf(item.url);

      const remove = document.createElement("button");
      remove.className = "remove";
      remove.textContent = "✕";
      remove.title = t("Remove");
      remove.addEventListener("click", async () => {
        await invoke("bookmark_remove", { url: item.url }).catch(() => {});
        items = items.filter((entry) => entry.url !== item.url);
        paint();
      });

      row.append(mark, title, host, remove);
      row.draggable = true;
      row.dataset.url = item.url;
      row.addEventListener("dragstart", (event) => {
        event.dataTransfer.setData("text/plain", item.url);
        event.dataTransfer.effectAllowed = "move";
        row.style.opacity = "0.5";
      });
      row.addEventListener("dragend", () => {
        row.style.opacity = "";
      });
      row.addEventListener("dragover", (event) => {
        event.preventDefault();
        row.style.borderTop = "2px solid var(--accent)";
      });
      row.addEventListener("dragleave", () => {
        row.style.borderTop = "";
      });
      row.addEventListener("drop", async (event) => {
        event.preventDefault();
        row.style.borderTop = "";
        const from = event.dataTransfer.getData("text/plain");
        if (!from || from === item.url) return;
        const index = items.findIndex((entry) => entry.url === item.url);
        if (index < 0) return;
        await invoke("bookmark_move", { from, to: index }).catch(() => {});
        const moved = items.find((entry) => entry.url === from);
        items = items.filter((entry) => entry.url !== from);
        items.splice(index, 0, moved);
        paint();
      });
      row.addEventListener("click", (event) => {
        if (event.target === remove) return;
        invoke("tab_navigate", { url: item.url }).catch(() => {});
      });
      list.append(row);
    }
  };

  search.addEventListener("input", paint);
  paint();
}

boot();
