// The chrome strip: tabs, address bar, toolbar, bookmarks and the key bridge the page forwards.
const { invoke } = window.__TAURI__.core;
const PAGE = Math.random().toString(36).slice(2, 6);
import { setLanguage, t } from "./i18n.js";
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const urlInput = document.getElementById("url");
const tabStrip = document.getElementById("tabs");

function reportError(what, error) {
  invoke("ui_log", {
    message: "ui error " + what + ": " + (error && error.message ? error.message : error),
  }).catch(() => {});
}
window.addEventListener("error", (event) =>
  reportError("uncaught", event.message + " @ " + (event.lineno || 0)),
);
window.addEventListener("unhandledrejection", (event) =>
  reportError("rejected", String(event.reason)),
);

let tabs = [];
let groups = [];
let dragging = null;
let settings = { show_bookmarks: false };
let draggingBookmark = null;
let selected = null;
let hidePill = null;
let hoverThumb = null;
let thumbOpen = false;
const downloadPill = document.getElementById("download");
let announcedDownload = 0;

function renderDownloads(tasks) {
  const latest = tasks && tasks.length ? tasks[tasks.length - 1] : null;
  if (!latest) {
    downloadPill.hidden = true;
    return;
  }
  downloadPill.hidden = false;
  downloadPill.dataset.id = String(latest.id);
  if (latest.state === "started" && latest.id !== announcedDownload) {
    announcedDownload = latest.id;
    const box = downloadPill.getBoundingClientRect();
    invoke("overlay_open_at", {
      kind: "downloads",
      x: Math.round(Math.min(box.left, window.innerWidth - 370)),
      y: Math.round(box.bottom + 4),
    }).catch(() => {});
  }
  downloadPill.title = latest.path || latest.url;
  if (latest.state === "started") {
    const mb = (latest.bytes || 0) / (1024 * 1024);
    const rate = latest.speed
      ? " " + (latest.speed / (1024 * 1024)).toFixed(1) + " MB/s"
      : "";
    downloadPill.textContent = mb > 0.05
      ? `\u2193 ${mb.toFixed(1)} MB${rate} ${latest.file_name}`
      : "\u2193 " + latest.file_name;
  } else if (latest.state === "finished") {
    downloadPill.textContent = "OK " + latest.file_name;
  } else {
    downloadPill.textContent = "X " + latest.file_name;
  }
  if (hidePill) clearTimeout(hidePill);
  if (latest.state !== "started") {
    hidePill = setTimeout(() => {
      downloadPill.hidden = true;
    }, 8000);
  }
}

const LIGHT_THEME = {
  bg: "#f4f5f7",
  panel: "#ffffff",
  border: "#d8dbe0",
  text: "#1b1d21",
  muted: "#5f6672",
  accent: "#2f6fed",
  tab: "#e7eaef",
  tabText: "#1b1d21",
  url: "#eef1f5",
};

let activeTheme = null;

function prefersLight() {
  return window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches;
}

function applyThemeMode(mode) {
  const root = document.documentElement;
  const light = mode === "light" || (mode === "system" && prefersLight());
  if (!light) {
    root.style.colorScheme = "dark";
    if (activeTheme) applyTheme(activeTheme);
    return;
  }
  root.style.colorScheme = "light";
  const map = {
    "--bg": LIGHT_THEME.bg,
    "--panel": LIGHT_THEME.panel,
    "--border": LIGHT_THEME.border,
    "--text": LIGHT_THEME.text,
    "--muted": LIGHT_THEME.muted,
    "--accent": LIGHT_THEME.accent,
    "--tab": LIGHT_THEME.tab,
    "--tab-text": LIGHT_THEME.text,
    "--hover": "#e4e8ee",
    "--url-bg": LIGHT_THEME.url,
  };
  for (const [key, value] of Object.entries(map)) root.style.setProperty(key, value);
}

function applyTheme(theme) {
  activeTheme = theme;
  const s = root.style;
  s.setProperty("--bg", theme.colors.bg);
  s.setProperty("--panel", theme.colors.chrome);
  s.setProperty("--accent", theme.colors.accent);
  s.setProperty("--text", theme.colors.text);
  s.setProperty("--border", theme.colors.border);
  s.setProperty("--radius", `${theme.radius}px`);
  s.setProperty("--font-size", `${theme.font_size}px`);
  s.setProperty("--tab", theme.colors.tab);
  s.setProperty("--tab-text", theme.colors.tab_text);
  s.setProperty("--url-bg", theme.colors.url_bg);
  s.setProperty("--hover", theme.colors.hover);
  s.setProperty("--font-family", theme.font_family);
  s.setProperty("--tab-radius", `${theme.tab_radius}px`);
}

window.__mixApplyTheme = applyTheme;

/** The one group a tab belongs to, if any. */
function groupOf(id) {
  return (groups || []).find((group) => group.members.includes(id));
}

const stripLeft = document.createElement("button");
const stripRight = document.createElement("button");
for (const [button, glyph, dir] of [
  [stripLeft, "\u2039", -1],
  [stripRight, "\u203a", 1],
]) {
  button.className = "tab-scroll";
  button.textContent = glyph;
  button.hidden = true;
  button.addEventListener("click", () => {
    tabStrip.scrollBy({ left: dir * Math.max(160, tabStrip.clientWidth * 0.6), behavior: "smooth" });
  });
}
tabStrip.parentElement.insertBefore(stripLeft, tabStrip);
tabStrip.after(stripRight);

function syncStripScroll() {
  const more = tabStrip.scrollWidth - tabStrip.clientWidth;
  const left = tabStrip.scrollLeft;
  stripLeft.hidden = more <= 1 || left <= 1;
  stripRight.hidden = more <= 1 || left >= more - 1;
}

tabStrip.addEventListener("scroll", syncStripScroll, { passive: true });
tabStrip.addEventListener(
  "wheel",
  (event) => {
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    if (tabStrip.scrollWidth <= tabStrip.clientWidth) return;
    event.preventDefault();
    tabStrip.scrollLeft += event.deltaY;
    syncStripScroll();
  },
  { passive: false },
);
window.addEventListener("resize", syncStripScroll);

const tabNodes = new Map();

/** The tab as it is now: handlers outlive several updates. */
function tabById(id) {
  return tabs.find((item) => item.id === id) || { id, title: "", url: "", alive: false };
}

function groupById(id) {
  return (groups || []).find((group) => group.id === id);
}

let groupMenuOpen = false;
let groupMenuUntil = 0;

/** Where a group chip is, in window coordinates for the overlay. */
function openGroupMenu(group, x, y) {
  groupMenuOpen = true;
  groupMenuUntil = Date.now() + 8000;
  clearTimeout(hoverThumb);
  hoverThumb = null;
  if (thumbOpen) {
    thumbOpen = false;
    invoke("overlay_close").catch(() => {});
  }
  invoke("overlay_open_at", {
    kind: "group_menu&id=" + group.id,
    x: Math.round(x - 12),
    y: Math.round(y),
  }).catch(() => {});
}

/** A closed group takes one slot in the strip, the way Edge shows them. */
function groupChip(group) {
  const el = document.createElement("div");
  el.dataset.group = group.id;
  el.className = "tab group-chip" + (group.collapsed ? " closed" : "");
  el.style.setProperty("--group", group.color || "#4c8dff");
  el.title = group.name;
  const dot = document.createElement("span");
  dot.className = "group-dot";
  const label = document.createElement("span");
  label.className = "tab-title";
  label.textContent = group.name;
  el.append(dot, label);
  if (group.collapsed) {
    const count = document.createElement("span");
    count.className = "group-count";
    count.textContent = String(group.members.length);
    el.append(count);
  }
  el.addEventListener("click", () => invoke("group_toggle", { id: group.id }).catch(() => {}));
  el.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    openGroupMenu(group, event.clientX, event.clientY);
  });
  el.addEventListener("dragover", (event) => {
    if (!dragging) return;
    event.preventDefault();
    el.classList.add("drop-target");
  });
  el.addEventListener("dragleave", () => el.classList.remove("drop-target"));
  el.addEventListener("drop", (event) => {
    event.preventDefault();
    el.classList.remove("drop-target");
    if (!dragging) return;
    invoke("group_add", { id: dragging, groupId: group.id }).catch(() => {});
  });
  return el;
}

let press = null;

function clearDropTargets() {
  for (const pill of tabStrip.querySelectorAll(".drop-target")) {
    pill.classList.remove("drop-target");
  }
}

function beginTabDrag(id, x, y) {
  press = { id, x, y, moved: false };
  const tab = tabNodes.get(id);
  if (!tab) return;
  tab.classList.add("pressing");
}

function onDragMove(event) {
  if (!press) return;
  if (!press.moved) {
    if (Math.abs(event.clientX - press.x) + Math.abs(event.clientY - press.y) < 6) return;
    press.moved = true;
    invoke("ui_log", { message: "tab drag start " + press.id }).catch(() => {});
    clearTimeout(hoverThumb);
    const source = tabNodes.get(press.id);
    if (source) {
      source.classList.remove("pressing");
      source.classList.add("dragging");
    }
    document.body.classList.add("tab-dragging");
  }
  clearDropTargets();
  const under = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest(".tab, .group-chip");
  if (!under || under.dataset.tab === undefined && !under.dataset.group) return;
  if (under.dataset.tab === press.id) return;
  under.classList.add("drop-target");
}

function onDragEnd(event) {
  const held = press;
  press = null;
  document.body.classList.remove("tab-dragging");
  if (!held) return;
  const source = tabNodes.get(held.id);
  if (source) source.classList.remove("dragging", "pressing");
  clearDropTargets();
  if (!held.moved) return;
  const under = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest(".tab, .group-chip");
  if (!under) {
    const strip = tabStrip.getBoundingClientRect();
    const outside =
      event.clientY - strip.bottom > 24 ||
      event.clientY < strip.top - 24 ||
      event.clientX < strip.left - 60 ||
      event.clientX > strip.right + 60;
    if (outside) {
      // The last tab of a window stays where it is: dragging it out would leave an
      // empty window behind, so the gesture is refused before it reaches the backend.
      if (tabs.length < 2) return;
      invoke("tab_pop_out", { id: held.id, x: event.screenX, y: event.screenY }).catch((error) =>
        invoke("ui_log", { message: "tab pop out failed: " + error }).catch(() => {}),
      );
      return;
    }
    let to = tabs.length;
    for (const pill of tabStrip.querySelectorAll(".tab[data-tab]")) {
      const box = pill.getBoundingClientRect();
      if (event.clientX < box.left + box.width / 2) {
        const index = tabs.findIndex((tab) => tab.id === pill.dataset.tab);
        if (index >= 0) to = index;
        break;
      }
    }
    const current = tabs.findIndex((tab) => tab.id === held.id);
    if (to !== current) invoke("tab_move", { id: held.id, to }).catch(() => {});
    return;
  }
  if (under.dataset.group) {
    invoke("group_add", { id: held.id, groupId: under.dataset.group }).catch(() => {});
    return;
  }
  const target = under.dataset.tab;
  if (!target || target === held.id) return;
  const to = tabs.findIndex((item) => item.id === target);
  invoke("ui_log", {
    message: `tab drag: ${held.id} -> ${target} at ${to}`,
  }).catch(() => {});
  if (to >= 0) {
    invoke("tab_move", { id: held.id, to }).catch((error) =>
      invoke("ui_log", { message: "tab drag failed: " + error }).catch(() => {}),
    );
  }
}

document.addEventListener("mousemove", onDragMove);
document.addEventListener("mouseup", onDragEnd);

/** Redraws the inside of a tab, keeping the element itself so drag and clicks live on. */
function fillTab(el, tab) {
  el.dataset.tab = tab.id;
  const classes = ["tab"];
  if (tab.id === selected) classes.push("active");
  if (!tab.alive && settings.mark_suspended !== false) classes.push("suspended");
  if (tab.incognito) classes.push("incognito");
  if (tab.pinned) classes.push("pinned");
  el.className = classes.join(" ");
  const group = groupOf(tab.id);
  el.style.setProperty("--group", group ? group.color || "#4c8dff" : "transparent");
  if (settings.tab_hover_preview) {
    el.setAttribute("aria-label", tab.title || tab.url);
    el.removeAttribute("title");
  } else {
    el.title = tab.alive ? tab.url : tab.url + "\n" + t("Suspended to save memory");
  }

  const label = document.createElement("span");
  label.className = "tab-title";
  label.textContent = tab.title || t(tab.incognito ? "Incognito" : "New tab");

  const close = document.createElement("button");
  close.className = "tab-close";
  close.textContent = "x";
  close.title = "Close tab";
  close.addEventListener("mousedown", (event) => event.stopPropagation());
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    invoke("tab_close", { id: tab.id });
  });

  const iconUrl = String(tab.icon || "");
  const looksReal =
    (iconUrl.startsWith("data:image/") && iconUrl.length > 64) ||
    iconUrl.startsWith("http://") ||
    iconUrl.startsWith("https://");
  const hasIcon = looksReal && !tab.loading;
  const icon = document.createElement(tab.loading ? "span" : hasIcon ? "img" : "span");
  icon.className = tab.loading ? "tab-icon spinner" : "tab-icon";
  if (hasIcon) {
    icon.src = tab.icon;
    icon.alt = "";
    icon.addEventListener("error", () => {
      icon.removeAttribute("src");
      icon.replaceWith(document.createElement("span"));
    });
  }

  if (tab.pinned) {
    const pin = document.createElement("span");
    pin.className = "tab-pin";
    pin.textContent = "\u25C6";
    pin.title = "Pinned";
    el.replaceChildren(icon, label, pin);
  } else if (settings.tab_show_close === false) {
    el.replaceChildren(icon, label);
  } else {
    el.replaceChildren(icon, label, close);
  }
}

/** Milliseconds of stillness before the card appears. */
const HOVER_DELAY = 650;

/**
 * Waits for the pointer to stop before opening the card.
 *
 * Every movement restarts the wait, so crossing the strip to reach another tab never
 * pops anything, while resting on one does - the timer measures stillness, not presence.
 * The earlier version cancelled on any movement past a few pixels, which a normal mouse
 * produces just by arriving on a tab, so the card never came up at all.
 */
function armHover(id, el) {
  if (groupMenuOpen) {
    if (Date.now() < groupMenuUntil) return;
    groupMenuOpen = false;
  }
  clearTimeout(hoverThumb);
  hoverThumb = setTimeout(async () => {
    hoverThumb = null;
    const box = el.getBoundingClientRect();
    const shown = await invoke("thumb_show", {
      id,
      x: box.left + box.width / 2,
    }).catch(() => false);
    if (shown) thumbOpen = true;
  }, HOVER_DELAY);
}

/** Wires the handlers a tab element keeps for its whole life. */
function wireTab(el, id) {
  el.draggable = false;
  el.addEventListener("dragstart", (event) => event.preventDefault());
  el.addEventListener("mousedown", (event) => {
    if (event.target.closest("button")) return;
    clearTimeout(hoverThumb);
    invoke("tab_select", { id });
  });
  el.addEventListener("dblclick", (event) => {
    if (settings.tab_double_click_close === false) return;
    event.preventDefault();
    invoke("tab_close", { id }).catch(() => {});
  });
  el.addEventListener("auxclick", (event) => {
    if (event.button !== 1) return;
    event.preventDefault();
    invoke("tab_pin", { id, pinned: !tabById(id).pinned }).catch(() => {});
  });
  el.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    invoke("tab_menu", { id, x: event.clientX, y: event.clientY }).catch(() => {});
  });
  el.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    beginTabDrag(id, event.clientX, event.clientY);
  });
  el.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    // A press on the close button is a click, not a drag: capturing the pointer here
    // would send that click to the tab instead of the button and swallow the close.
    if (event.target && event.target.closest && event.target.closest("button")) return;
    try {
      el.setPointerCapture(event.pointerId);
    } catch {
      // Capture is a best effort: without it the drag simply stops at the strip.
    }
  });
  if (settings.tab_hover_preview) {
    el.addEventListener("mouseenter", (enter) => {
      void enter;
      armHover(id, el);
    });
    el.addEventListener("mousemove", () => armHover(id, el));
    el.addEventListener("mouseleave", () => {
      clearTimeout(hoverThumb);
      hoverThumb = null;
      if (thumbOpen) {
        thumbOpen = false;
        invoke("overlay_close").catch(() => {});
      }
    });
  }
}

/** 110ms of fade for a tab that has just appeared. */
function enterStrip(el) {
  if (!el.animate) return;
  el.animate([{ opacity: 0, transform: "translateY(-3px)" }, { opacity: 1, transform: "none" }], {
    duration: 110,
    easing: "ease-out",
  });
}

/** The same in reverse, then the element leaves the document. */
function leaveStrip(el) {
  if (!el.animate) {
    el.remove();
    return;
  }
  const done = el.animate([{ opacity: 1 }, { opacity: 0 }], {
    duration: 100,
    easing: "ease-in",
  });
  done.finished.then(() => el.remove()).catch(() => el.remove());
}

let progressWasOn = false;

/** The bar under the address row, and the reload button that becomes a stop. */
function syncLoading() {
  const active = tabs.find((tab) => tab.id === selected);
  const loading = Boolean(active && active.loading);
  const bar = document.getElementById("progress");
  if (bar) bar.classList.toggle("on", loading);
  if (loading) progressWasOn = true;
  const reload = document.querySelector('[data-nav="reload"], [data-nav="stop"]');
  if (reload) {
    reload.dataset.nav = loading ? "stop" : "reload";
    reload.textContent = loading ? "\u00d7" : "R";
    reload.title = loading ? t("Stop loading") : t("Reload");
  }
}

function renderTabs() {
  const order = [];
  const placed = new Set();
  for (const tab of tabs) {
    if (placed.has(tab.id)) continue;
    const group = groupOf(tab.id);
    if (!group) {
      order.push({ kind: "tab", id: tab.id });
      placed.add(tab.id);
      continue;
    }
    if (placed.has(group.id)) continue;
    placed.add(group.id);
    order.push({ kind: "group", id: group.id });
    if (!group.collapsed) {
      for (const member of group.members) {
        if (placed.has(member)) continue;
        placed.add(member);
        order.push({ kind: "tab", id: member });
      }
    }
  }

  const keep = new Set();
  for (const item of order) {
    if (item.kind === "group") {
      const group = groupById(item.id);
      if (!group) continue;
      const key = "group:" + group.id;
      keep.add(key);
      let el = tabNodes.get(key);
      if (!el) {
        el = groupChip(group);
        tabNodes.set(key, el);
      } else {
        el.className = "tab group-chip" + (group.collapsed ? " closed" : "");
        el.style.setProperty("--group", group.color || "#4c8dff");
        const title = el.querySelector(".tab-title");
        if (title) title.textContent = group.name;
        let count = el.querySelector(".group-count");
        if (group.collapsed) {
          if (!count) {
            count = document.createElement("span");
            count.className = "group-count";
            el.append(count);
          }
          count.textContent = String(group.members.length);
        } else if (count) {
          count.remove();
        }
      }
      tabStrip.append(el);
      continue;
    }

    const tab = tabs.find((entry) => entry.id === item.id);
    if (!tab) continue;
    keep.add(tab.id);
    let el = tabNodes.get(tab.id);
    if (!el) {
      el = document.createElement("div");
      wireTab(el, tab.id);
      tabNodes.set(tab.id, el);
      enterStrip(el);
    }
    fillTab(el, tab);
    tabStrip.append(el);
  }

  syncStripScroll();
  syncLoading();

  for (const [key, el] of tabNodes) {
    if (keep.has(key)) continue;
    if (key.startsWith("group:")) {
      const group = groupById(key.slice(6));
      if (!group) {
        el.remove();
        tabNodes.delete(key);
      }
      continue;
    }
    if (!tabs.some((tab) => tab.id === key)) {
      leaveStrip(el);
      tabNodes.delete(key);
    }
  }
}

let translateOn = false;

async function runTranslate() {
  await invoke("translate_page_inplace").catch(() => {});
  for (let attempt = 0; attempt < 40; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 400));
    const [count, error] = await invoke("translate_status").catch(() => [0, null]);
    if (error) return false;
    if (count > 0) return true;
  }
  return false;
}

async function toggleTranslate() {
  if (!selected) return;

  const already = await invoke("translate_state").catch(() => false);
  if (already) {
    await invoke("translate_restore").catch(() => {});
    translateOn = false;
    renderToolbar(currentExtensions);
    return;
  }

  translateOn = await runTranslate();
  if (!translateOn) {
    await new Promise((resolve) => setTimeout(resolve, 700));
    translateOn = await runTranslate();
  }
  translateOn = await invoke("translate_state").catch(() => translateOn);
  renderToolbar(currentExtensions);
}

const SEARCH_ENGINES = {
  baidu: "https://www.baidu.com/s?wd=",
  bing: "https://www.bing.com/search?q=",
  google: "https://www.google.com/search?q=",
  ddg: "https://duckduckgo.com/?q=",
};

function looksLikeAddress(text) {
  const value = (text || "").trim();
  if (!value) return null;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return value;
  if (value.startsWith("mix://")) {
    const page = value.slice(6).split("/")[0];
    return "http://tauri.localhost/pages/" + page + ".html";
  }
  if (value.includes(" ") || !value.includes(".")) return null;
  return "https://" + value;
}

function addressFor(text) {
  const direct = looksLikeAddress(text);
  if (direct) return direct;
  const query = (text || "").trim();
  const template = (settings.custom_search || "").trim();
  if (template.includes("%s")) {
    return template.replace("%s", encodeURIComponent(query));
  }
  const base = SEARCH_ENGINES[settings.search_engine] || SEARCH_ENGINES.baidu;
  return base + encodeURIComponent(query);
}

function openSearchPanel(kind) {
  const width = 560;
  const x = Math.max(8, Math.round((window.innerWidth - width) / 2));
  invoke("overlay_open_at", { kind, x, y: 64 }).catch(() => {});
}

function openPalette() {
  openSearchPanel("palette");
}

function openTabSearch() {
  openSearchPanel("tab_search");
}

const audioState = {};

async function takeScreenshot() {
  try {
    const path = await invoke("tab_screenshot");
    const name = String(path).split(/[\\/]/).pop();
    showToast(t("Screenshot saved") + ": " + name);
  } catch (error) {
    showToast(String(error));
  }
}

const TOOLBAR_ITEMS = {
  home: { glyph: "\u2302", label: "Home", where: "outside", run: () => invoke("shortcut", { action: "open_home" }) },
  mute: { glyph: "\u{1F508}", label: "Mute tab", where: "outside", run: () => invoke("tab_mute") },
  shot: { glyph: "\u{1F4F7}", label: "Take a screenshot", where: "outside", run: () => takeScreenshot() },
  reopen: { glyph: "\u21ba", label: "Reopen closed tab", where: "outside", run: () => invoke("shortcut", { action: "reopen_tab" }) },
  translate: { glyph: "\u6587", label: "Translate this page", where: "inside", run: () => toggleTranslate() },
  zoom: {
    glyph: "\u2315",
    label: "Zoom",
    where: "inside",
    run: (button) => {
      const box = button.getBoundingClientRect();
      // The panel centers on the control it belongs to.
      return invoke("overlay_toggle", {
        kind: "zoom",
        x: Math.round(box.left + box.width / 2),
        y: Math.round(box.bottom + 4),
      });
    },
  },
  find: { glyph: "\u2315", label: "Find in page", where: "outside", run: () => invoke("overlay_toggle", { kind: "find" }) },
  bookmark: { glyph: "\u2606", label: "Bookmark this page", where: "inside", run: () => invoke("bookmark_toggle") },
  extensions: { glyph: "\u25a4", label: "Extensions", where: "outside", run: () => invoke("overlay_open", { kind: "app_menu&page=extensions" }) },
  downloads: { glyph: "\u2193", label: "Downloads", where: "outside", run: () => invoke("open_dir", { which: "downloads" }) },
};

let currentExtensions = [];

async function renderToolbar(extensions) {
  if (extensions) currentExtensions = extensions;
  const saved = new Set((await invoke("bookmark_list")).map((item) => item.url));
  const active = tabs.find((tab) => tab.id === selected);
  const bookmarked = active ? saved.has(active.url) : false;

  const inside = document.getElementById("url-actions");
  const outside = document.getElementById("toolbar");
  const inner = [];
  const outer = [];

  const order = (settings.toolbar && settings.toolbar.length
    ? settings.toolbar
    : ["translate", "zoom", "bookmark", "extensions", "downloads"]);
  for (const id of order) {
    if (!(settings.toolbar || []).includes(id)) continue;
    const spec = TOOLBAR_ITEMS[id];
    if (!spec) continue;
    const button = document.createElement("button");
    button.className = spec.where === "inside" ? "url-action" : "tool-btn";
    button.title = spec.label;
    if (id === "translate") {
      const on = translateOn;
      button.textContent = spec.glyph;
      button.title = t(on ? "Show original" : spec.label);
      button.classList.toggle("is-on", Boolean(on));
    } else if (id === "bookmark") {
      button.textContent = bookmarked ? "\u2605" : "\u2606";
    } else if (id === "zoom") {
      // The button keeps its icon: the level belongs in the panel and the tooltip.
      const level = await invoke("zoom_get").catch(() => 1);
      button.textContent = spec.glyph;
      button.title = `${t("Zoom")} ${Math.round(level * 100)}%`;
    } else if (id === "mute") {
      const now = audioState[selected] || { playing: false, muted: false };
      button.textContent = now.muted ? "\u{1F507}" : now.playing ? "\u{1F50A}" : "\u{1F508}";
      button.title = t(now.muted ? "Unmute tab" : "Mute tab");
      button.classList.toggle("is-on", Boolean(now.muted));
    } else {
      button.textContent = spec.glyph;
    }
    button.addEventListener("click", () => spec.run(button).catch(() => {}));
    (spec.where === "inside" ? inner : outer).push(button);
  }

  const hidden = new Set(settings.hidden_extensions || []);
  for (const ext of (extensions || []).filter((item) => item.status === "loaded" && item.icon)) {
    if (hidden.has(ext.name)) continue;
    const button = document.createElement("button");
    button.className = "tool-btn ext-btn";
    button.title = ext.version ? `${ext.name} ${ext.version}` : ext.name;
    const img = document.createElement("img");
    img.src = ext.icon;
    img.alt = "";
    button.append(img);
    button.addEventListener("click", () =>
      invoke("overlay_open", { kind: `ext_info&name=${encodeURIComponent(ext.name)}` }),
    );
    outer.unshift(button);
  }

  const weight = (node) => {
    if (node.title === t(TOOLBAR_ITEMS.extensions.label)) return 1;
    if (node.title === t(TOOLBAR_ITEMS.downloads.label)) return 2;
    return 0;
  };
  outer.sort((left, right) => weight(left) - weight(right));
  inside.replaceChildren(...inner);
  outside.hidden = outer.length === 0;
  outside.replaceChildren(...outer);
}


function renderBookmarks(list) {
  const bar = document.getElementById("bookmarks");
  const items = list || [];
  bar.hidden = !settings.show_bookmarks;
  if (bar.hidden) return;

  bar.replaceChildren(
    ...items.map((bookmark, index) => {
      const chip = document.createElement("button");
      chip.className = "bookmark";
      chip.textContent = bookmark.title || bookmark.url;
      chip.title = bookmark.url;
      chip.draggable = true;
      chip.addEventListener("click", () => invoke("tab_new", { url: bookmark.url }));
      chip.addEventListener("dragstart", (event) => {
        draggingBookmark = bookmark.url;
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", bookmark.url);
        chip.classList.add("dragging");
      });
      chip.addEventListener("dragend", () => {
        draggingBookmark = null;
        chip.classList.remove("dragging");
      });
      chip.addEventListener("dragover", (event) => {
        if (!draggingBookmark) return;
        event.preventDefault();
        chip.classList.add("drop-target");
      });
      chip.addEventListener("dragleave", () => chip.classList.remove("drop-target"));
      chip.addEventListener("drop", (event) => {
        event.preventDefault();
        chip.classList.remove("drop-target");
        const source = draggingBookmark;
        if (!source || source === bookmark.url) return;
        invoke("bookmark_move", { from: source, to: index }).catch(() => {});
      });
      return chip;
    }),
  );

  if (!items.length) {
    const hint = document.createElement("span");
    hint.className = "bookmark-hint";
    hint.textContent = "Drag a link here to bookmark it";
    bar.append(hint);
  }
}

/** True only for an image source that will actually draw something. */
function realIcon(url) {
  const value = String(url || "");
  return (
    (value.startsWith("data:image/") && value.length > 64) ||
    value.startsWith("http://") ||
    value.startsWith("https://")
  );
}

function syncAddressIcon(active) {
  const holder = document.getElementById("url-icon");
  if (!holder) return;
  if (active && realIcon(active.icon)) {
    holder.src = active.icon;
    holder.hidden = false;
  } else {
    holder.removeAttribute("src");
    holder.hidden = true;
  }
}

function prettyUrl(url) {
  const section = url.match(/pages\/([a-z]+)\.html#(.+)$/i);
  if (section) {
    return "mix://" + section[1].toLowerCase();
  }
  const prefix = "http://tauri.localhost/pages/";
  if (url.startsWith(prefix)) {
    return "mix://" + url.slice(prefix.length).replace(/\.html$/, "");
  }
  if (url === "about:blank") return "";
  return url;
}

function syncUrl() {
  if (document.activeElement === urlInput) return;
  const active = tabs.find((tab) => tab.id === selected);
  const next = active ? prettyUrl(active.url) : "";
  if (next && urlInput.value !== next) urlInput.value = next;
}

function windowTag() {
  const id = selected || (tabs[0] && tabs[0].id) || "";
  return id.split("-tab-")[0];
}

function setTabs(payload) {
  // Every window receives every tab event, so a list that belongs to another window is
  // dropped: drawing it would put another window's tabs in this strip.
  const mine = window.__mixWindow;
  if (payload.window && mine && payload.window !== mine) {
    report("ui tabs ignored " + payload.window + " page=" + PAGE + " own=" + mine);
    return;
  }
  tabs = payload.tabs || [];
  groups = payload.groups || [];
  selected = payload.selected || null;
  try {
    renderTabs();
  } catch (error) {
    reportError("renderTabs", error);
  }
  const active = tabs.find((tab) => tab.id === selected);
  if (active) document.title = active.title || "Mix";
  document.body.classList.toggle("incognito", Boolean(active && active.incognito));
  syncAddressIcon(active);
  syncUrl();
}

const report = (message) => invoke("ui_log", { message }).catch(() => {});

let translateError = "";

function showToast(text) {
  const toast = document.getElementById("toast");
  if (!toast) return;
  toast.textContent = text;
  toast.hidden = false;
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => {
    toast.hidden = true;
  }, 7000);
}

window.__mixTranslateFailed = (message) => {
  translateError = String(message || "failed");
  showToast(`${t("Translate failed")}: ${t(translateError)}`);
  renderToolbar(currentExtensions);
};
report("chrome boot v2");
setInterval(() => report("chrome alive"), 60000);

window.__mixUrlAction = (action) => {
  urlInput.focus();
  const selection = urlInput.value.slice(urlInput.selectionStart, urlInput.selectionEnd);
  const place = (text) => {
    urlInput.setRangeText(text, urlInput.selectionStart, urlInput.selectionEnd, "end");
    urlInput.dispatchEvent(new Event("input"));
  };
  if (action === "cut") {
    invoke("clipboard_write", { text: selection }).catch(() => {});
    place("");
  } else if (action === "copy") {
    invoke("clipboard_write", { text: selection }).catch(() => {});
  } else if (action === "paste") {
    invoke("clipboard_read")
      .then((text) => place(text || ""))
      .catch(() => {});
  } else if (action === "select-all") {
    urlInput.select();
  }
};

const ctxMenu = document.getElementById("ctx-menu");

function hideContextMenu() {
  if (!ctxMenu) return;
  ctxMenu.hidden = true;
  ctxMenu.replaceChildren();
}

function showContextMenu(x, y, rows) {
  if (!ctxMenu) return;
  ctxMenu.replaceChildren(
    ...rows.map((row) => {
      if (row === "-") {
        return Object.assign(document.createElement("div"), { className: "ctx-divider" });
      }
      const button = document.createElement("button");
      button.className = "ctx-item";
      button.textContent = row.label;
      button.disabled = Boolean(row.disabled);
      button.addEventListener("click", () => {
        hideContextMenu();
        row.run();
      });
      return button;
    }),
  );
  ctxMenu.hidden = false;
  const width = ctxMenu.offsetWidth || 180;
  const height = ctxMenu.offsetHeight || 200;
  ctxMenu.style.left = Math.min(x, window.innerWidth - width - 6) + "px";
  ctxMenu.style.top = Math.min(y, window.innerHeight - height - 6) + "px";
}

function urlEditMenu() {
  const selection = urlInput.value.slice(urlInput.selectionStart, urlInput.selectionEnd);
  const read = () => navigator.clipboard.readText();
  const place = (text) => {
    const start = urlInput.selectionStart;
    const end = urlInput.selectionEnd;
    urlInput.setRangeText(text, start, end, "end");
    urlInput.dispatchEvent(new Event("input"));
  };

  return [
    {
      label: "Cut",
      disabled: !selection,
      run: () => {
        navigator.clipboard.writeText(selection).catch(() => {});
        place("");
      },
    },
    {
      label: "Copy",
      disabled: !selection,
      run: () => navigator.clipboard.writeText(selection).catch(() => {}),
    },
    { label: "Paste", run: () => read().then(place).catch(() => {}) },
    "-",
    {
      label: "Select all",
      run: () => {
        urlInput.focus();
        urlInput.select();
      },
    },
  ];
}

function chromeMenu() {
  return [
    { label: "New tab", run: () => invoke("tab_new", { url: null }) },
    { label: "Reload", run: () => invoke("tab_nav", { action: "reload" }) },
    "-",
    { label: "Settings", run: () => invoke("overlay_open", { kind: "app_menu&page=settings" }) },
  ];
}

document.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  const inField = event.target === urlInput || Boolean(event.target.closest("#url-wrap"));
  invoke("chrome_menu", {
    kind: inField ? "url" : "chrome",
    x: event.clientX,
    y: event.clientY,
  }).catch(() => {});
});

document.addEventListener("mousedown", (event) => {
  if (!event.target.closest("#ctx-menu")) hideContextMenu();
  if (document.activeElement === urlInput && !event.target.closest("#url-wrap")) {
    urlInput.blur();
    invoke("focus_page").catch(() => {});
  }
});
window.addEventListener("blur", hideContextMenu);
window.addEventListener("resize", hideContextMenu);

window.__mixSettingsChanged = async () => {
  settings = await invoke("settings_get");
  renderToolbar(await invoke("ext_list"));
  renderBookmarks(await invoke("bookmark_list"));
};

async function boot() {
  applyTheme(await invoke("theme_get"));
  settings = await invoke("settings_get");
  setLanguage(settings.language || "en");

  await listen("tabs:changed", (event) => {
    setTabs(event.payload);
    invoke("translate_state")
      .then((on) => {
        translateOn = Boolean(on);
        return invoke("ext_list");
      })
      .then((list) => renderToolbar(list))
      .catch(() => {});
  });
  await listen("bookmarks:changed", async () => {
    renderBookmarks(await invoke("bookmark_list"));
    renderToolbar(await invoke("ext_list"));
  });
  await listen("ext:changed", async () => renderToolbar(await invoke("ext_list")));
  await listen("tab:audio", (event) => {
    const payload = event.payload || {};
    if (!payload.id) return;
    audioState[payload.id] = { playing: Boolean(payload.playing), muted: Boolean(payload.muted) };
    renderToolbar();
  });
  await listen("settings:changed", async () => {
    settings = await invoke("settings_get");
    renderBookmarks(await invoke("bookmark_list"));
    renderToolbar(await invoke("ext_list"));

  });
  await listen("tab:loading", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.loading = event.payload.loading;
    renderTabs();
    if (tab.id === selected) syncLoading();
  });
  await listen("tab:icon", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.icon = event.payload.icon;
    renderTabs();
    syncAddressIcon(tabs.find((item) => item.id === selected));
  });
  await listen("tab:title", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.title = event.payload.title;
    renderTabs();
    if (tab.id === selected) document.title = tab.title || "Mix";
  });
  await listen("tab:url", (event) => {
    const tab = tabs.find((item) => item.id === event.payload.id);
    if (!tab) return;
    tab.url = event.payload.url;
    if (tab.id === selected) syncUrl();
  });
  await listen("theme:changed", (event) => applyTheme(event.payload));
  await listen("download:changed", (event) => renderDownloads(event.payload));
  setTabs(await invoke("tab_list"));

  applyThemeMode(settings.theme_mode || "dark");
  const scheme = window.matchMedia && window.matchMedia("(prefers-color-scheme: light)");
  if (scheme && scheme.addEventListener) {
    scheme.addEventListener("change", () => {
      if ((settings.theme_mode || "dark") === "system") applyThemeMode("system");
    });
  }

  const fitStrip = () => {
    const bar = document.getElementById("bookmarks");
    const barHeight = bar && !bar.hidden ? bar.getBoundingClientRect().height : 0;
    const height = Math.round(document.body.getBoundingClientRect().height - barHeight);
    if (height > 0) invoke("chrome_fit", { height }).catch(() => {});
  };
  fitStrip();
  window.addEventListener("resize", fitStrip);
  report(`boot tabs=${tabs.length} selected=${selected}`);

  await listen("ui:palette", () => openPalette());
  await listen("ui:tab-search", () => openTabSearch());
  await listen("ui:new-tab", () => invoke("tab_new", {}));
  await listen("ui:open-url", (event) => invoke("tab_new", { url: event.payload }));
  await listen("url:commit", () => {
    invoke("tab_navigate", { url: addressFor(urlInput.value) }).catch(flagBadUrl);
    urlInput.blur();
  });
  await listen("url:cancel", () => {
    syncUrl();
    urlInput.blur();
  });
  await listen("ui:focus-url", () => {
    urlInput.focus();
    urlInput.select();
    if (typeof feedSuggest === "function") feedSuggest(urlInput.value);
  });
  const bar = document.getElementById("bookmarks");
  bar.addEventListener("dragover", (event) => {
    event.preventDefault();
    bar.classList.add("drop-target");
  });
  bar.addEventListener("dragleave", () => bar.classList.remove("drop-target"));
  bar.addEventListener("drop", (event) => {
    event.preventDefault();
    bar.classList.remove("drop-target");
    const url = event.dataTransfer.getData("text/plain");
    if (!url || !url.startsWith("http")) return;
    invoke("bookmark_add", { url, title: url }).catch(() => {});
  });

  renderDownloads(await invoke("download_list"));
  renderToolbar(await invoke("ext_list"));
  renderBookmarks(await invoke("bookmark_list"));

  for (const button of document.querySelectorAll("[data-window]")) {
    button.addEventListener("click", () => invoke("window_control", { action: button.dataset.window }));
  }
  for (const button of document.querySelectorAll("[data-nav]")) {
    button.addEventListener("click", () => invoke("tab_nav", { action: button.dataset.nav }));
  }
  for (const handle of document.querySelectorAll("[data-resize]")) {
    handle.addEventListener("mousedown", (event) => {
      event.preventDefault();
      invoke("window_resize", { direction: handle.dataset.resize });
    });
  }

  const drag = document.getElementById("drag");
  drag.addEventListener("mousedown", (event) => {
    if (event.target.closest("button") || event.target.closest(".tab")) return;
    invoke("window_drag");
  });
  drag.addEventListener("dblclick", (event) => {
    if (event.target.closest("button") || event.target.closest(".tab")) return;
    invoke("window_control", { action: "max" });
  });

  window.addEventListener("keydown", (event) => {
    if (event.altKey && event.shiftKey && event.key.startsWith("Arrow")) {
      event.preventDefault();
      invoke("snap", { direction: event.key.replace("Arrow", "").toLowerCase() });
      return;
    }
    let action = null;
    if (event.key === "F5") action = "reload";
    else if (event.key === "F11") action = "fullscreen";
    else if (event.key === "F12") action = "devtools";
    else if (event.ctrlKey || event.metaKey) {
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      switch (key) {
        case "t": action = event.shiftKey ? "reopen_tab" : "new_tab"; break;
        case "w": action = "close_tab"; break;
        case "l": action = "focus_url"; break;
        case "r": action = "reload"; break;
        case "Tab": action = event.shiftKey ? "prev_tab" : "next_tab"; break;
        case "=":
        case "+": action = "zoom_in"; break;
        case "-": action = "zoom_out"; break;
        case "0": action = "zoom_reset"; break;
        case "p": action = event.shiftKey ? "palette" : "print"; break;
        case "f": action = "find"; break;
        case "j": action = "downloads"; break;
        case "d": action = "bookmark"; break;
        case "n": action = event.shiftKey ? "new_incognito" : "new_window"; break;
        default: action = null;
      }
    }
    if (!action) return;
    event.preventDefault();
    if (action === "focus_url") {
      urlInput.focus();
      urlInput.select();
      return;
    }
    invoke("shortcut", { action });
  }, true);

  var suggestOpen = false;
  var suggestItems = [];
  var suggestIndex = -1;
  var suggestQuery = "";
  var takeSuggestions = (query) => (items) => {
    suggestItems = Array.isArray(items) ? items : [];
    suggestIndex = -1;
    suggestQuery = query;
  };
  var restoreQuery = () => {
    if (suggestIndex < 0) return;
    suggestIndex = -1;
    urlInput.value = suggestQuery;
    urlInput.setSelectionRange(urlInput.value.length, urlInput.value.length);
  };
  var moveSuggestion = (step) => {
    if (!suggestItems.length) return;
    let next = suggestIndex + step;
    if (next < 0) next = suggestItems.length - 1;
    if (next >= suggestItems.length) next = 0;
    suggestIndex = next;
    const item = suggestItems[next];
    urlInput.value = item.url;
    urlInput.setSelectionRange(item.url.length, item.url.length);
    invoke("suggest_active", { index: next }).catch(() => {});
  };
  var resetSuggestion = () => {
    suggestIndex = -1;
    invoke("suggest_active", { index: -1 }).catch(() => {});
  };
  var feedSuggest = (text) => {
    if (!suggestOpen) {
      suggestOpen = true;
      const box = urlInput.getBoundingClientRect();
      invoke("suggest_open", { x: box.left, width: box.width })
        .then(() => invoke("suggest_feed", { text }))
        .then(takeSuggestions(text))
        .catch((error) => {
          suggestOpen = false;
          report(`suggest open failed: ${error}`);
        });
      return;
    }
    invoke("suggest_feed", { text }).then(takeSuggestions(text)).catch(() => {});
  };
  const closeSuggest = () => {
    if (!suggestOpen) return;
    suggestOpen = false;
    suggestItems = [];
    resetSuggestion();
    invoke("overlay_close").catch(() => {});
  };

  urlInput.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      moveSuggestion(event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key.length === 1 || event.key === "Backspace" || event.key === "Delete") {
      restoreQuery();
      feedSuggest(urlInput.value);
    }
  });
  urlInput.addEventListener("input", () => feedSuggest(urlInput.value));

  const urlInsert = (text) => {
    restoreQuery();
    const start = urlInput.selectionStart ?? urlInput.value.length;
    const end = urlInput.selectionEnd ?? start;
    if (text === "\b") {
      if (start !== end) urlInput.setRangeText("", start, end, "end");
      else if (start > 0) urlInput.setRangeText("", start - 1, start, "end");
    } else {
      urlInput.setRangeText(text, start, end, "end");
    }
    urlInput.dispatchEvent(new Event("input"));
  };

  const urlKey = (key) => {
    urlInput.focus();
    const start = urlInput.selectionStart ?? 0;
    const end = urlInput.selectionEnd ?? start;
    const move = (at) => urlInput.setSelectionRange(at, at);
    if (key === "down" || key === "up") {
      moveSuggestion(key === "down" ? 1 : -1);
      return;
    }
    if (key === "left") move(end > start ? start : Math.max(0, start - 1));
    else if (key === "right") move(end > start ? end : Math.min(urlInput.value.length, end + 1));
    else if (key === "home") move(0);
    else if (key === "end") move(urlInput.value.length);
    else if (key === "delete") {
      if (start !== end) urlInput.setRangeText("", start, end, "end");
      else if (end < urlInput.value.length) urlInput.setRangeText("", end, end + 1, "end");
      urlInput.dispatchEvent(new Event("input"));
    } else if (key === "select-all") {
      urlInput.select();
    } else if (key === "delete-word") {
      const boundary = /[\s/.:?&=#@_-]/;
      let at = start;
      while (at > 0 && !boundary.test(urlInput.value[at - 1])) at -= 1;
      while (at > 0 && boundary.test(urlInput.value[at - 1])) at -= 1;
      if (at === start && start > 0) at = start - 1;
      urlInput.setRangeText("", at, Math.max(end, start), "end");
      urlInput.dispatchEvent(new Event("input"));
    } else if (key === "copy" || key === "cut") {
      const selected = urlInput.value.slice(start, end);
      if (selected) {
        invoke("clipboard_write", { text: selected }).catch(() => {});
        if (key === "cut") {
          urlInput.setRangeText("", start, end, "end");
          urlInput.dispatchEvent(new Event("input"));
        }
      }
    } else if (key === "paste") {
      invoke("clipboard_read")
        .then((text) => {
          if (text) urlInsert(String(text).replace(/[\r\n]+/g, " "));
        })
        .catch(() => {});
    }
  };

  window.__mixUrlInsert = urlInsert;
  window.__mixUrlKey = urlKey;
  await listen("ui:url-insert", (event) => urlInsert(event.payload || ""));
  await listen("ui:url-key", (event) => urlKey(event.payload || ""));
  urlInput.addEventListener("blur", () => closeSuggest());

  let badUrlTimer = null;
  const flagBadUrl = () => {
    urlInput.classList.add("invalid");
    urlInput.title = "Not a web address";
    if (badUrlTimer) clearTimeout(badUrlTimer);
    badUrlTimer = setTimeout(() => {
      urlInput.classList.remove("invalid");
      urlInput.title = "";
    }, 1600);
  };

  urlInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      invoke("tab_navigate", { url: addressFor(urlInput.value) }).catch(flagBadUrl);
      closeSuggest();
      urlInput.blur();
    }
    if (event.key === "Escape") {
      closeSuggest();
      syncUrl();
      urlInput.blur();
    }
  });


  downloadPill.addEventListener("click", () => {
    const box = downloadPill.getBoundingClientRect();
    invoke("overlay_open_at", {
      kind: "downloads",
      x: Math.round(Math.min(box.left, window.innerWidth - 370)),
      y: Math.round(box.bottom + 4),
    }).catch(() => {});
  });

  document.getElementById("theme-btn").addEventListener("click", () => invoke("overlay_toggle", { kind: "app_menu" }));

  // A press in the strip that is not a control dismisses an open overlay. The page half
  // of this lives in the injected script; the overlay is its own webview, so clicks in
  // the toolbar, the tab strip and the address bar never reach it.
  document.addEventListener(
    "pointerdown",
    (event) => {
      const target = event.target;
      // A button is left alone: it may open or toggle an overlay of its own, and closing
      // it here would be undone by the click that follows. Anything else dismisses.
      if (target && target.closest && target.closest("button")) return;
      invoke("overlay_close").catch(() => {});
    },
    true,
  );
  document
    .getElementById("tabs")
    .addEventListener("auxclick", (event) => {
      if (event.button !== 1) return;
      const pill = event.target.closest(".tab");
      if (pill && pill.dataset.tab) invoke("tab_close", { id: pill.dataset.tab });
    });
  document.getElementById("tab-new").addEventListener("click", () => {
    report("ui new tab clicked page=" + PAGE + " own=" + (window.__mixWindow || windowTag()));
    invoke("tab_new", {}).catch(() => {});
  });

  document.addEventListener(
    "click",
    (event) => {
      const button = event.target.closest("[data-action='screenshot']");
      if (!button || !event.ctrlKey) return;
      event.preventDefault();
      event.stopPropagation();
      invoke("tab_screenshot", { full: true }).catch(() => {});
    },
    true,
  );

}

boot().catch((error) => reportError("boot", error));