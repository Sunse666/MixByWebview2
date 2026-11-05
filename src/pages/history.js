import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");
let language = "en";

function navItem(glyph, label, active) {
  const item = document.createElement("div");
  item.className = "nav-item" + (active ? " active" : "");
  item.innerHTML = `<span>${glyph}</span><span></span>`;
  item.lastElementChild.textContent = t(label);
  nav.append(item);
  return item;
}

function dayLabel(at) {
  const date = new Date(at * 1000);
  const today = new Date();
  const sameDay = date.toDateString() === today.toDateString();
  if (sameDay) return t("Today") + " - " + date.toLocaleDateString();
  return date.toLocaleDateString();
}

function hostOf(url) {
  try {
    return new URL(url).host || url;
  } catch (error) {
    return url;
  }
}

function render(visits, filter) {
  main.replaceChildren();

  const head = document.createElement("h2");
  head.textContent = t("History");
  main.append(head);

  const bar = document.createElement("div");
  bar.className = "toolbar";
  const search = document.createElement("input");
  search.placeholder = t("Search history");
  search.value = filter || "";
  const clear = document.createElement("button");
  clear.className = "button";
  clear.textContent = t("Clear browsing data");
  bar.append(search, clear);
  main.append(bar);

  search.addEventListener("input", () => paint(visits, search.value));
  clear.addEventListener("click", async () => {
    await invoke("history_clear").catch(() => {});
    paint([], "");
  });

  const list = document.createElement("div");
  list.id = "list";
  main.append(list);
  paint(visits, filter || "");
}

function paint(all, filter) {
  const list = document.getElementById("list");
  if (!list) return;
  list.replaceChildren();

  const needle = filter.trim().toLowerCase();
  const rows = all.filter(
    (visit) =>
      !needle ||
      visit.url.toLowerCase().includes(needle) ||
      (visit.title || "").toLowerCase().includes(needle),
  );

  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = t("Nothing recorded yet.");
    list.append(empty);
    return;
  }

  let current = null;
  let bucket = null;
  for (const visit of rows) {
    const label = dayLabel(visit.at);
    if (label !== current) {
      current = label;
      const day = document.createElement("div");
      day.className = "day";
      const title = document.createElement("h3");
      title.textContent = label;
      bucket = document.createElement("div");
      day.append(title, bucket);
      list.append(day);
    }

    const row = document.createElement("div");
    row.className = "entry";
    const time = document.createElement("span");
    time.className = "time";
    time.textContent = new Date(visit.at * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
    const title = document.createElement("span");
    title.className = "title";
    title.textContent = visit.title || visit.url;
    const host = document.createElement("span");
    host.className = "host";
    host.textContent = hostOf(visit.url);
    const remove = document.createElement("button");
    remove.className = "remove";
    remove.textContent = "✕";
    remove.title = t("Remove");
    remove.addEventListener("click", () => row.remove());

    row.append(time, title, host, remove);
    row.addEventListener("dblclick", () => invoke("tab_new", { url: visit.url }).catch(() => {}));
    bucket.append(row);
  }
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  language = (settings && settings.language) || "en";
  setLanguage(language);

  navItem("⟲", "History", true);
  navItem("☆", "Bookmarks", false);
  navItem("⧉", "Extensions", false);
  navItem("⚙", "Settings", false);

  const visits = await invoke("history_list", { limit: 500 }).catch(() => []);
  render(visits || [], "");
}

boot();
