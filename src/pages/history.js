import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

const PAGES = {
  bookmarks: "http://tauri.localhost/pages/bookmarks.html",
  extensions: "http://tauri.localhost/pages/extensions.html",
  settings: "http://tauri.localhost/pages/settings.html",
};

let visits = [];
const selection = new Set();

function keyOf(visit) {
  return visit.url + "|" + visit.at;
}

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

function dayLabel(at) {
  const date = new Date(at * 1000);
  return date.toDateString() === new Date().toDateString()
    ? t("Today") + " - " + date.toLocaleDateString()
    : date.toLocaleDateString();
}

function hostOf(url) {
  try {
    return new URL(url).host || url;
  } catch (error) {
    return url;
  }
}

function initialOf(url) {
  const host = hostOf(url).replace(/^www\./, "");
  return (host[0] || "?").toUpperCase();
}

function matches(visit, needle) {
  return (
    !needle ||
    visit.url.toLowerCase().includes(needle) ||
    (visit.title || "").toLowerCase().includes(needle)
  );
}

const PAGE_GLYPHS = {
  "pages/history.html": "⟲",
  "pages/bookmarks.html": "☆",
  "pages/extensions.html": "⧉",
  "pages/settings.html": "⚙",
  "pages/newtab.html": "+",
};

function iconFor(visit) {
  const holder = document.createElement("span");
  holder.className = "fav";

  const internal = Object.keys(PAGE_GLYPHS).find((page) => visit.url.includes(page));
  if (internal) {
    holder.textContent = PAGE_GLYPHS[internal];
    holder.classList.add("glyph");
    return holder;
  }

  if (visit.icon) {
    const img = document.createElement("img");
    img.src = visit.icon;
    img.alt = "";
    img.addEventListener("error", () => {
      holder.replaceChildren(document.createTextNode(initialOf(visit.url)));
      holder.classList.add("glyph");
    });
    holder.append(img);
    return holder;
  }

  holder.textContent = initialOf(visit.url);
  holder.classList.add("glyph");
  return holder;
}

function rowFor(visit, repaint) {
  const row = document.createElement("div");
  row.className = "entry";

  const box = document.createElement("input");
  box.type = "checkbox";
  box.className = "pick";
  box.checked = selection.has(keyOf(visit));
  box.addEventListener("change", () => {
    if (box.checked) selection.add(keyOf(visit));
    else selection.delete(keyOf(visit));
    repaint();
  });

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
  remove.addEventListener("click", async () => {
    // A stale row must not stay visible when the backend refuses the delete.
    visits = visits.filter((item) => keyOf(item) !== keyOf(visit));
    selection.delete(keyOf(visit));
    paintSites();
    repaint();
    await invoke("history_remove", { entries: [[visit.url, visit.at]] }).catch(() => {});
  });

  row.append(box, iconFor(visit), time, title, host, remove);
  // A single click opens the entry, like the address list in a desktop browser.
  row.addEventListener("click", (event) => {
    if (event.target === box || event.target === remove) return;
    invoke("tab_navigate", { url: visit.url }).catch(() => {});
  });
  return row;
}

function groupRows(rows, repaint) {
  const holder = document.createElement("div");
  let current = null;
  let bucket = null;

  for (const visit of rows) {
    const label = dayLabel(visit.at);
    if (label !== current) {
      current = label;
      const day = document.createElement("div");
      day.className = "day";
      const head = document.createElement("h3");
      head.textContent = label;
      bucket = document.createElement("div");
      day.append(head, bucket);
      holder.append(day);
    }
    bucket.append(rowFor(visit, repaint));
  }

  return holder;
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("History");

  navItem("⟲", "History", true, null);
  navItem("☆", "Bookmarks", false, PAGES.bookmarks);
  navItem("⧉", "Extensions", false, PAGES.extensions);
  navItem("⚙", "Settings", false, PAGES.settings);

  const head = document.createElement("h2");
  head.textContent = t("History");
  main.append(head);

  const bar = document.createElement("div");
  bar.className = "toolbar";
  const search = document.createElement("input");
  search.placeholder = t("Search history");
  const clearAll = document.createElement("button");
  clearAll.className = "button";
  clearAll.textContent = t("Clear browsing data");
  bar.append(search, clearAll);

  const actions = document.createElement("div");
  actions.className = "toolbar";
  actions.style.marginTop = "-6px";
  const selectAll = document.createElement("button");
  selectAll.className = "button";
  selectAll.textContent = t("Select all");
  const removeSelected = document.createElement("button");
  removeSelected.className = "button";
  removeSelected.disabled = true;
  removeSelected.textContent = t("Delete selected");
  actions.append(selectAll, removeSelected);

  const sites = document.createElement("div");
  sites.className = "toolbar";
  sites.id = "sites";

  const datalist = document.createElement("datalist");
  datalist.id = "history-suggest";
  search.setAttribute("list", "history-suggest");

  const list = document.createElement("div");
  main.append(bar, sites, datalist, actions, list);

  let siteFilter = "";

  const visible = () => {
    const needle = search.value.trim().toLowerCase();
    return visits
      .filter((visit) => matches(visit, needle))
      .filter((visit) => !siteFilter || hostOf(visit.url) === siteFilter);
  };

  const paintSites = () => {
    const counts = new Map();
    for (const visit of visits) {
      const host = hostOf(visit.url);
      if (!host || host.startsWith("tauri.localhost")) continue;
      counts.set(host, (counts.get(host) || 0) + 1);
    }
    const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8);

    sites.replaceChildren();
    if (!top.length) return;

    const chip = (label, value) => {
      const button = document.createElement("button");
      button.className = "button";
      button.textContent = label;
      if (value === siteFilter) button.style.borderColor = "var(--accent)";
      button.addEventListener("click", () => {
        siteFilter = siteFilter === value ? "" : value;
        paintSites();
        paint();
      });
      return button;
    };

    sites.append(chip(t("All sites"), ""));
    for (const [host, count] of top) sites.append(chip(host + " (" + count + ")", host));
  };

  const paintSuggestions = () => {
    const seen = new Set();
    datalist.replaceChildren(
      ...visits
        .filter((visit) => {
          const key = (visit.title || visit.url).toLowerCase();
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        })
        .slice(0, 30)
        .map((visit) => {
          const option = document.createElement("option");
          option.value = visit.title || visit.url;
          return option;
        }),
    );
  };

  const paint = () => {
    list.replaceChildren();
    removeSelected.disabled = selection.size === 0;
    removeSelected.textContent =
      selection.size > 0
        ? t("Delete selected") + " (" + selection.size + ")"
        : t("Delete selected");

    const rows = visible();
    if (!rows.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = t("Nothing recorded yet.");
      list.append(empty);
      return;
    }
    list.append(groupRows(rows, paint));
  };

  search.addEventListener("input", paint);

  selectAll.addEventListener("click", () => {
    const rows = visible();
    const allSelected = rows.length > 0 && rows.every((visit) => selection.has(keyOf(visit)));
    for (const visit of rows) {
      if (allSelected) selection.delete(keyOf(visit));
      else selection.add(keyOf(visit));
    }
    paint();
  });

  removeSelected.addEventListener("click", async () => {
    const entries = [...selection].map((item) => {
      const [url, at] = item.split("|");
      return [url, Number(at)];
    });
    await invoke("history_remove", { entries }).catch(() => {});
    visits = visits.filter((visit) => !selection.has(keyOf(visit)));
    selection.clear();
    paint();
  });

  clearAll.addEventListener("click", async () => {
    await invoke("history_clear").catch(() => {});
    visits = [];
    selection.clear();
    paint();
  });

  const reload = async () => {
    const fresh = (await invoke("history_list", { limit: 500 }).catch(() => [])) || [];
    const same =
      fresh.length === visits.length &&
      fresh.every((visit, index) => keyOf(visit) === keyOf(visits[index]));
    if (same) return;
    visits = fresh;
    paintSuggestions();
    paintSites();
    paint();
  };

  await reload();



  // The page stays open while other tabs record visits, so poll for changes.
  setInterval(reload, 1500);
  window.addEventListener("focus", reload);
}

boot();
