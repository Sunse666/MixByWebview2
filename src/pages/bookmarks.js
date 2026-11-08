import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const invoke = withFeedback(window.__TAURI__.core.invoke);

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

  // Folder names are derived from the bookmarks themselves, so no extra store.
  const foldersOf = (list) => [...new Set(list.map((item) => item.folder || ""))].filter(Boolean);

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

      const startRename = () => {
        const input = document.createElement("input");
        input.className = "hex";
        input.value = title.textContent;
        input.style.width = "320px";
        const finish = async (save) => {
          if (!save) {
            input.replaceWith(title);
            return;
          }
          const next = input.value.trim();
          input.replaceWith(title);
          if (!next || next === title.textContent) return;
          await invoke("bookmark_update", { url: item.url, title: next }).catch(() => {});
          title.textContent = next;
        };
        input.addEventListener("keydown", (event) => {
          if (event.key === "Enter") finish(true);
          if (event.key === "Escape") finish(false);
        });
        input.addEventListener("blur", () => finish(true));
        title.replaceWith(input);
        input.focus();
        input.select();
      };
      title.title = t("Double click to rename");
      title.addEventListener("dblclick", (event) => {
        event.stopPropagation();
        startRename();
      });

      const folderSelect = document.createElement("select");
      folderSelect.className = "button";
      folderSelect.style.minWidth = "96px";
      const none = document.createElement("option");
      none.value = "";
      none.textContent = t("No folder");
      folderSelect.append(none);
      for (const name of foldersOf(items)) {
        const option = document.createElement("option");
        option.value = name;
        option.textContent = name;
        if ((item.folder || "") === name) option.selected = true;
        folderSelect.append(option);
      }
      folderSelect.addEventListener("click", (event) => event.stopPropagation());
      folderSelect.addEventListener("change", async () => {
        await invoke("bookmark_set_folder", {
          url: item.url,
          folder: folderSelect.value,
        }).catch(() => {});
        item.folder = folderSelect.value;
        paintName();
      });

      const remove = document.createElement("button");
      remove.className = "remove";
      remove.textContent = "✕";
      remove.title = t("Remove");
      remove.addEventListener("click", async () => {
        await invoke("bookmark_remove", { url: item.url }).catch(() => {});
        items = items.filter((entry) => entry.url !== item.url);
        paint();
      });

      const paintName = () => {
        const group = item.folder || "";
        host.textContent = group ? group + " · " + hostOf(item.url) : hostOf(item.url);
      };
      paintName();

      row.append(mark, title, host, folderSelect, remove);
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

  const newFolder = document.createElement("input");
  newFolder.className = "hex";
  newFolder.style.width = "160px";
  newFolder.placeholder = t("New folder");
  newFolder.addEventListener("keydown", async (event) => {
    if (event.key !== "Enter") return;
    const name = newFolder.value.trim();
    if (!name) return;
    // Folders exist once a bookmark joins them, so move the first visible row.
    const first = items[0];
    if (first) {
      await invoke("bookmark_set_folder", { url: first.url, folder: name }).catch(() => {});
      first.folder = name;
      newFolder.value = "";
      paint();
    }
  });
  main.insertBefore(newFolder, list);

  search.addEventListener("input", paint);
  paint();

}

boot();