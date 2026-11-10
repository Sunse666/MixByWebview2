import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const invoke = withFeedback(window.__TAURI__.core.invoke);

const nav = document.getElementById("nav");
const main = document.getElementById("main");

const PAGES = {
  history: "http://tauri.localhost/pages/history.html",
  bookmarks: "http://tauri.localhost/pages/bookmarks.html",
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

function size(bytes) {
  if (bytes > 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  if (bytes > 1024) return Math.round(bytes / 1024) + " KB";
  return bytes + " B";
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Downloads");

  navItem("⟲", "History", false, PAGES.history);
  navItem("☆", "Bookmarks", false, PAGES.bookmarks);
  navItem("⧉", "Extensions", false, PAGES.extensions);
  navItem("⇩", "Downloads", true, null);
  navItem("⚙", "Settings", false, PAGES.settings);

  const head = document.createElement("h2");
  head.textContent = t("Downloads");
  main.append(head);

  const bar = document.createElement("div");
  bar.className = "toolbar";
  const folder = document.createElement("button");
  folder.className = "button";
  folder.textContent = t("Open downloads folder");
  folder.addEventListener("click", () => invoke("open_dir", { which: "downloads" }).catch(() => {}));
  const clear = document.createElement("button");
  clear.className = "button";
  clear.textContent = t("Clear list");
  clear.addEventListener("click", async () => {
    await invoke("download_clear").catch(() => {});
    paint(tasks);
  });
  bar.append(folder, clear);
  main.append(bar);

  const list = document.createElement("div");
  main.append(list);

  let tasks = [];

  const paint = (items) => {
    list.replaceChildren();
    if (!items.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("No downloads.");
      list.append(empty);
      return;
    }

    for (const task of items) {
      const row = document.createElement("div");
      row.className = "entry";

      const name = document.createElement("span");
      name.className = "title";
      name.textContent = task.file_name || task.url;

      const state = document.createElement("span");
      state.className = "host";
      const parts = [t(task.state) || task.state];
      if (task.bytes) parts.push(size(task.bytes));
      // A rate only means something while the file is still growing.
      if (task.state === "started" && task.speed) parts.push(size(task.speed) + "/s");
      if (task.total && task.state !== "finished") {
        parts.push(Math.min(100, Math.round((task.bytes / task.total) * 100)) + "%");
      }
      state.textContent = parts.join(" · ");

      if (task.state === "started" || task.state === "interrupted") {
        const pause = document.createElement("button");
        pause.className = "button";
        pause.textContent =
          t(task.paused || task.state === "interrupted" ? "Resume" : "Pause");
        pause.addEventListener("click", async () => {
          await invoke("download_pause", { id: task.id, paused: !task.paused }).catch(() => {});
        });
        const cancel = document.createElement("button");
        cancel.className = "button";
        cancel.textContent = t("Cancel");
        cancel.addEventListener("click", async () => {
          await invoke("download_cancel", { id: task.id }).catch(() => {});
        });
        row.append(name, state, pause, cancel);
        list.append(row);
        return;
      }

      const open = document.createElement("button");
      open.className = "button";
      open.textContent = t("Open");
      open.addEventListener("click", () => invoke("download_open", { id: task.id }).catch(() => {}));

      const reveal = document.createElement("button");
      reveal.className = "button";
      reveal.textContent = t("Show in folder");
      reveal.addEventListener("click", () => invoke("download_reveal", { id: task.id }).catch(() => {}));

      row.append(name, state, open, reveal);
      list.append(row);
    }
  };

  tasks = (await invoke("download_list").catch(() => [])) || [];
  paint(tasks);

  // progress arrives while the page is open
  setInterval(async () => {
    const fresh = (await invoke("download_list").catch(() => [])) || [];
    if (JSON.stringify(fresh) !== JSON.stringify(tasks)) {
      tasks = fresh;
      paint(tasks);
    }
  }, 1000);
}

boot();