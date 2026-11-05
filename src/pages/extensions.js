import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

const PAGES = {
  history: "http://tauri.localhost/pages/history.html",
  bookmarks: "http://tauri.localhost/pages/bookmarks.html",
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

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");

  navItem("⟲", "History", false, PAGES.history);
  navItem("☆", "Bookmarks", false, PAGES.bookmarks);
  navItem("⧉", "Extensions", true, null);
  navItem("⚙", "Settings", false, PAGES.settings);

  const head = document.createElement("h2");
  head.textContent = t("Extensions");
  main.append(head);

  const restartBar = document.createElement("div");
  restartBar.className = "toolbar";
  restartBar.hidden = true;
  const restartHint = document.createElement("span");
  restartHint.className = "card-note";
  restartHint.textContent = t("Restart to apply");
  const restart = document.createElement("button");
  restart.className = "button";
  restart.textContent = t("Restart now");
  restart.addEventListener("click", () => invoke("app_restart").catch(() => {}));
  restartBar.append(restartHint, restart);
  main.append(restartBar);

  const grid = document.createElement("div");
  grid.className = "grid";
  main.append(grid);

  const list = (await invoke("ext_list").catch(() => [])) || [];
  const loaded = list.filter((ext) => ext.status === "loaded");

  if (!loaded.length) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = t("No extensions loaded.");
    main.append(empty);
    return;
  }

  const hidden = new Set(settings.hidden_extensions || []);
  let disabledNow = [...(settings.disabled_extensions || [])];

  for (const ext of loaded) {
    const card = document.createElement("div");
    card.className = "card";
    const note2 = document.createElement("div");
    note2.className = "card-note";
    note2.textContent = t("Restart to apply");

    const top = document.createElement("div");
    top.className = "card-head";
    if (ext.icon) {
      const img = document.createElement("img");
      img.src = ext.icon;
      img.alt = "";
      top.append(img);
    }
    const title = document.createElement("div");
    title.className = "card-title";
    title.textContent = ext.version ? ext.name + " " + ext.version : ext.name;
    top.append(title);
    card.append(top);

    const note = document.createElement("div");
    note.className = "card-note";
    note.textContent = ext.description || t("No description");
    card.append(note);

    const meta = document.createElement("div");
    meta.className = "card-note";
    meta.textContent = "MV" + ext.manifest_version + " · " + ext.dir;
    card.append(meta);

    if (ext.permissions && ext.permissions.length) {
      const perms = document.createElement("div");
      perms.className = "card-note";
      perms.textContent = t("Permissions") + ": " + ext.permissions.slice(0, 6).join(", ");
      card.append(perms);
    }

    const actions = document.createElement("div");
    actions.className = "card-actions";

    const folder = document.createElement("button");
    folder.className = "button";
    folder.textContent = t("Open folder");
    folder.addEventListener("click", () => invoke("open_dir", { which: "extensions" }).catch(() => {}));
    actions.append(folder);

    const spacer = document.createElement("div");
    spacer.className = "spacer";
    actions.append(spacer);

    const bar = document.createElement("button");
    bar.className = "button";
    const syncBar = () => {
      bar.textContent = hidden.has(ext.name) ? t("Show in the toolbar") : t("Hide from the toolbar");
    };
    syncBar();
    bar.addEventListener("click", async () => {
      if (hidden.has(ext.name)) hidden.delete(ext.name);
      else hidden.add(ext.name);
      syncBar();
      await invoke("settings_set", { patch: { hidden_extensions: [...hidden] } }).catch(() => {});
    });
    actions.append(bar);

    const toggle = document.createElement("button");
    toggle.className = "switch";
    toggle.setAttribute("role", "switch");
    toggle.setAttribute("aria-checked", "true");
    toggle.title = t("Enabled");
    toggle.addEventListener("click", async () => {
      const on = toggle.getAttribute("aria-checked") === "true";
      toggle.setAttribute("aria-checked", String(!on));
      const disabled = new Set(disabledNow);
      if (on) disabled.add(ext.source || ext.name);
      else disabled.delete(ext.source || ext.name);
      disabledNow = [...disabled];
      await invoke("settings_set", { patch: { disabled_extensions: [...disabled] } }).catch(() => {});
      restartBar.hidden = false;
    });
    actions.append(toggle);

    card.append(actions);
    grid.append(card);
  }
}

boot();
