// The sidebar: bookmarks, downloads and history in one column.
import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const { listen } = window.__TAURI__.event;

const invoke = withFeedback(window.__TAURI__.core.invoke);

const tabs = document.getElementById("tabs");
const body = document.getElementById("body");

const VIEWS = ["bookmarks", "downloads", "history", "tabs"];
const LABELS = { bookmarks: "Bookmarks", downloads: "Downloads", history: "History", tabs: "Tabs" };
let current = "bookmarks";
let tabSnapshot = null;

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

  if (current === "tabs") {
    const list = tabSnapshot || (await invoke("tab_list").catch(() => null));
    return renderTabs(list);
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

function renderTabs(list) {
  body.replaceChildren();
  const items = (list && list.tabs) || [];
  if (!items.length) return body.append(empty(t("No tabs")));
  for (const item of items) {
    const line = row(item.title || item.url, hostOf(item.url), item.icon, () =>
      invoke("tab_select", { id: item.id }).catch(() => {}),
    );
    line.addEventListener("auxclick", (event) => {
      if (event.button !== 1) return;
      event.preventDefault();
      invoke("tab_close", { id: item.id }).catch(() => {});
    });
    if (list.selected === item.id) line.classList.add("on");
    body.append(line);
  }
}

window.__mixSidebarSide = (value) => {
  side = value === "right" ? "right" : "left";
  placeGrip();
};

window.__mixSidebarTabs = (list) => {
  tabSnapshot = list;
  if (current === "tabs") renderTabs(list);
};

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

function widthAt(state, event) {
  const raw = state.width + (event.screenX - state.x) * state.dir;
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

grip.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  grip.setPointerCapture(event.pointerId);
  document.body.classList.add("resizing");
  drag = { x: event.screenX, width: 0, dir: 1, from: 0, moves: 0, ready: false };
  const current = drag;
  invoke("settings_get")
    .then((settings) => {
      if (!settings || drag !== current) return;
      side = settings.sidebar === "right" ? "right" : "left";
      placeGrip();
      current.width = settings.sidebar_width || 260;
      current.from = current.width;
      current.dir = side === "right" ? -1 : 1;
      current.ready = true;
    })
    .catch(() => {});
});

grip.addEventListener("pointermove", (event) => {
  if (!drag || !drag.ready) return;
  drag.moves += 1;
  schedule(widthAt(drag, event));
});

function endDrag(event) {
  if (!drag) return;
  const state = drag;
  drag = null;
  frame = 0;
  document.body.classList.remove("resizing");
  if (!state.ready) {
    invoke("ui_log", { message: "sidebar resize: the grip let go before the settings arrived" }).catch(() => {});
    return;
  }
  invoke("sidebar_resize", { width: widthAt(state, event), persist: true }).catch(() => {});
}

grip.addEventListener("pointerup", endDrag);
grip.addEventListener("pointercancel", endDrag);
grip.addEventListener("lostpointercapture", () => {
  if (!drag) return;
  invoke("ui_log", { message: "sidebar resize: the pointer was taken away (" + side + ")" }).catch(() => {});
  drag = null;
  frame = 0;
  document.body.classList.remove("resizing");
});

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
    button.textContent = t(LABELS[view]);
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
