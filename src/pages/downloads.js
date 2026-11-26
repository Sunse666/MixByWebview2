import { setLanguage, t } from "../i18n.js";
import { toast, withFeedback, setFeedbackText } from "./feedback.js";

const invoke = withFeedback(window.__TAURI__.core.invoke);

/**
 * Runs a command that acts on a file, and says why when it cannot.
 *
 * These used to end in `.catch(() => {})`, so a file the reader had moved or deleted gave a
 * silent nothing on "Open" and looked like a dead button. The reason the backend returns is
 * the useful part.
 */
function acts(command, args) {
  invoke(command, args).catch((error) => {
    const detail = (error && (error.message || error.code)) || String(error);
    toast(String(detail), true);
  });
}

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

    // Newest first: the list on disk is in the order the files were started, and the one
    // just fetched is the one worth seeing.
    for (const task of items.slice().reverse()) {
      const row = document.createElement("div");
      row.className = "entry";

      const name = document.createElement("span");
      name.className = "title";
      name.textContent = task.file_name || task.url;

      const state = document.createElement("span");
      state.className = "host";
      // A queued download has moved no bytes, which from here looks exactly like one that is
      // running slowly. Saying which it is turns "is it broken?" into "wait".
      const parts = [t(task.queued ? "Waiting for a slot" : task.state) || task.state];
      if (task.bytes) parts.push(size(task.bytes));
      if (task.state === "started" && task.speed) parts.push(size(task.speed) + "/s");
      if (task.total && task.state !== "finished") {
        parts.push(Math.min(100, Math.round((task.bytes / task.total) * 100)) + "%");
      }
      // Why it failed, in the words the backend chose: "this server needs you to sign in
      // first" is the difference between a retry worth making and one that is not.
      if (task.state === "failed" && task.error) parts.push(t(task.error) || task.error);
      state.textContent = parts.join(" · ");

      if (task.state === "started" || task.state === "interrupted" || task.state === "paused") {
        // Both stopped states keep their bytes, so both offer the same button - but only
        // one of them was asked for, and the row should say which.
        const stopped = task.paused || task.state === "interrupted" || task.state === "paused";
        const pause = document.createElement("button");
        pause.className = "button";
        pause.textContent = t(stopped ? "Resume" : "Pause");
        pause.addEventListener("click", async () => {
          await invoke("download_pause", { id: task.id, paused: !stopped }).catch((error) => {
            toast(String((error && (error.message || error.code)) || error), true);
          });
        });
        const cancel = document.createElement("button");
        cancel.className = "button";
        cancel.textContent = t("Cancel");
        cancel.addEventListener("click", async () => {
          await invoke("download_cancel", { id: task.id }).catch(() => {});
        });
        row.append(name, state, pause, cancel);
        list.append(row);
        // Not `return`: that ended the whole list at the first download still in flight.
        continue;
      }

      if (task.state === "failed") {
        // Nothing to open: the file is partial at best. Offer the download again instead.
        //
        // Through `download_retry`, which continues the row that is already there. This used
        // to call `download_url`, and that made a second row for the same file, lost the
        // bytes the first attempt had written, and left the failed row beside its replacement.
        const retry = document.createElement("button");
        retry.className = "button";
        retry.textContent = t("Try again");
        retry.addEventListener("click", async () => {
          await invoke("download_retry", { id: task.id }).catch((error) => {
            toast(String((error && (error.message || error.code)) || error), true);
          });
        });
        const reveal = document.createElement("button");
        reveal.className = "button";
        reveal.textContent = t("Show in folder");
        reveal.addEventListener("click", () => acts("download_reveal", { id: task.id }));
        row.append(name, state, retry, reveal);
        list.append(row);
        continue;
      }

      const open = document.createElement("button");
      open.className = "button";
      open.textContent = t("Open");
      open.addEventListener("click", () => acts("download_open", { id: task.id }));

      const reveal = document.createElement("button");
      reveal.className = "button";
      reveal.textContent = t("Show in folder");
      reveal.addEventListener("click", () => acts("download_reveal", { id: task.id }));

      row.append(name, state, open, reveal);
      list.append(row);
    }
  };

  tasks = (await invoke("download_list").catch(() => [])) || [];
  paint(tasks);

  setInterval(async () => {
    const fresh = (await invoke("download_list").catch(() => [])) || [];
    if (JSON.stringify(fresh) !== JSON.stringify(tasks)) {
      tasks = fresh;
      paint(tasks);
    }
  }, 1000);
}

boot();