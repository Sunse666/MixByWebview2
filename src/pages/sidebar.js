// The sidebar: bookmarks, downloads and history in one column.
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
          invoke("download_open", { id: item.id }).catch(() => {}),
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

const grip = document.getElementById("grip");
const MIN_WIDTH = 180;
const MAX_WIDTH = 640;
let side = "left";
let drag = null;
let frame = 0;
let pending = 0;

function placeGrip() {
  grip.classList.toggle("right", side !== "right");
  grip.classList.toggle("left", side === "right");
}

function widthAt(event) {
  const raw = drag.width + (event.screenX - drag.x) * drag.dir;
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(raw)));
}

function schedule(width) {
  pending = width;
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    invoke("sidebar_resize", { width: pending, persist: false }).catch(() => {});
  });
}

grip.addEventListener("pointerdown", async (event) => {
  if (event.button !== 0) return;
  const settings = await invoke("settings_get").catch(() => null);
  if (!settings) return;
  side = settings.sidebar === "right" ? "right" : "left";
  placeGrip();
  drag = {
    x: event.screenX,
    width: settings.sidebar_width || 260,
    dir: side === "right" ? -1 : 1,
  };
  grip.setPointerCapture(event.pointerId);
  document.body.classList.add("resizing");
  event.preventDefault();
});

grip.addEventListener("pointermove", (event) => {
  if (!drag) return;
  schedule(widthAt(event));
});

function endDrag(event) {
  if (!drag) return;
  const width = widthAt(event);
  drag = null;
  frame = 0;
  document.body.classList.remove("resizing");
  invoke("sidebar_resize", { width, persist: true }).catch(() => {});
}

grip.addEventListener("pointerup", endDrag);
grip.addEventListener("pointercancel", endDrag);

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  side = settings && settings.sidebar === "right" ? "right" : "left";
  placeGrip();
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
  await listen("bookmarks:changed", () => paint()).catch(() => {});
  await listen("download:changed", () => paint()).catch(() => {});
  await paint();
}

boot();
