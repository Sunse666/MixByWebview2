// The sidebar: bookmarks, downloads and history in one narrow column.
//
// It is its own webview so the page next to it keeps its own layout, and it asks for
// the same data as the internal pages do rather than duplicating any of it.
import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const { listen } = window.__TAURI__.event;

const invoke = withFeedback(window.__TAURI__.core.invoke);

const tabs = document.getElementById("tabs");
const body = document.getElementById("body");

const VIEWS = ["bookmarks", "downloads", "history"];
let current = "bookmarks";

function row(label, host, icon, run) {
  const line = document.createElement("div");
  line.className = "row";
  if (icon) {
    const image = document.createElement("img");
    image.src = icon;
    image.alt = "";
    line.append(image);
  }
  const text = document.createElement("span");
  text.className = "label";
  text.textContent = label;
  line.append(text);
  if (host) {
    const where = document.createElement("span");
    where.className = "host";
    where.textContent = host;
    line.append(where);
  }
  line.addEventListener("click", () => run());
  return line;
}

function empty(message) {
  const note = document.createElement("div");
  note.className = "empty";
  note.textContent = message;
  return note;
}

async function paint() {
  for (const button of tabs.children) {
    button.classList.toggle("on", button.dataset.view === current);
  }
  body.replaceChildren();

  if (current === "bookmarks") {
    const items = (await invoke("bookmark_list").catch(() => [])) || [];
    if (!items.length) return body.append(empty(t("No bookmarks yet")));
    for (const item of items) {
      body.append(
        row(item.title || item.url, hostOf(item.url), item.icon, () =>
          invoke("tab_navigate", { url: item.url }).catch(() => {}),
        ),
      );
    }
    return;
  }

  if (current === "downloads") {
    const items = (await invoke("download_list").catch(() => [])) || [];
    if (!items.length) return body.append(empty(t("No downloads")));
    for (const item of items) {
      const name = item.fileName || item.url || "";
      body.append(
        row(name, item.state || "", null, () =>
          invoke("download_open", { path: item.path }).catch(() => {}),
        ),
      );
    }
    return;
  }

  const visits = (await invoke("history_list", { limit: 50 }).catch(() => [])) || [];
  if (!visits.length) return body.append(empty(t("No history yet")));
  const seen = new Set();
  for (const visit of visits) {
    if (seen.has(visit.url)) continue;
    seen.add(visit.url);
    body.append(
      row(visit.title || visit.url, hostOf(visit.url), visit.icon, () =>
        invoke("tab_navigate", { url: visit.url }).catch(() => {}),
      ),
    );
  }
}

function hostOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  setFeedbackText(t("Saved"), t("Could not save:"));

  for (const view of VIEWS) {
    const button = document.createElement("button");
    button.className = "side-tab";
    button.dataset.view = view;
    button.textContent = t(view === "bookmarks" ? "Bookmarks" : view === "downloads" ? "Downloads" : "History");
    button.addEventListener("click", () => {
      current = view;
      paint();
    });
    tabs.append(button);
  }
  // Keep the lists current: a bookmark saved in a tab, a finished download or a new
  // visit should appear here without reopening the sidebar.
  await listen("bookmarks:changed", () => paint()).catch(() => {});
  await listen("download:changed", () => paint()).catch(() => {});
  await paint();
}

boot();
