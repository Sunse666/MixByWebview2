import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";

const invoke = withFeedback(window.__TAURI__.core.invoke);

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
  document.title = t("Extensions");

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

  // Extensions WebView2 actually loaded: toggling here takes effect at once.
  const live = (await invoke("extension_live_list").catch(() => [])) || [];
  if (live.length) {
    const head = document.createElement("h2");
    head.textContent = t("Loaded by the browser");
    head.style.margin = "0 0 10px";
    main.append(head);
    const liveNote = document.createElement("div");
    liveNote.className = "card-note";
    liveNote.style.margin = "0 0 8px";
    liveNote.textContent = t("What the browser actually loaded, built into it included; the switches are on the cards below");
    main.append(liveNote);

    const liveCard = document.createElement("div");
    liveCard.className = "card";
    liveCard.style.maxWidth = "720px";
    liveCard.style.marginBottom = "18px";

    for (const item of live) {
      const line = document.createElement("div");
      line.className = "entry";
      const name = document.createElement("span");
      name.className = "title";
      name.textContent = item.name || item.id;
      const id = document.createElement("span");
      id.className = "host";
      id.textContent = item.id;

      // Status only: the switch belongs on the card below, which carries the folder
      // and can switch an extension back on as well as off. Two switches doing the
      // same thing in two places was the confusing part of this page.
      const state = document.createElement("span");
      state.className = "card-note";
      state.textContent = item.enabled ? t("Enabled") : t("Disabled");

      line.append(name, id, state);
      liveCard.append(line);
    }
    main.append(liveCard);
  }

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
  const enabledDirs = new Set();

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

    // One control, both directions: turning it off asks the browser to unload the
    // extension, turning it on hands the browser the folder again, which is the only
    // handle left once an extension has been switched off.
    // Matching by name is the only handle we have; compare loosely so a difference in
    // case or spacing does not read as "not loaded".
    const norm = (value) => String(value || "").toLowerCase().replace(/\s+/g, "");
    const liveEntry = live.find((item) => norm(item.name) === norm(ext.name))
      || live.find((item) => norm(item.name).includes(norm(ext.name)))
      || live.find((item) => norm(ext.name).includes(norm(item.name)) && norm(item.name).length > 3);
    // The live list is asked first; when it says nothing about an extension, the state
    // the browser confirmed when it was switched is used instead, so the switch does
    // not fall back to "off" on every visit.
    const remembered = (settings.extension_state || {})[ext.dir];
    const enabledNow =
      liveEntry && liveEntry.enabled !== undefined
        ? Boolean(liveEntry.enabled)
        : Boolean(remembered) || enabledDirs.has(ext.dir);
    const toggle = document.createElement("button");
    toggle.className = "switch";
    toggle.setAttribute("role", "switch");
    toggle.setAttribute("aria-checked", String(enabledNow));
    toggle.title = t("Enabled");
    toggle.addEventListener("click", async () => {
      const on = toggle.getAttribute("aria-checked") === "true";
      toggle.disabled = true;
      if (on) {
        enabledDirs.delete(ext.dir);
        const done = await invoke("extension_live_set", {
          id: liveEntry.id,
          enabled: false,
          dir: ext.dir,
        })
          .then(() => true)
          .catch(() => false);
        toggle.disabled = false;
        if (!done) {
          enabledDirs.add(ext.dir);
          note2.textContent = t("The browser refused that; restart and try again");
          return;
        }
        toggle.setAttribute("aria-checked", "false");
        note2.textContent = t("Disabled");
        return;
      }
      const added = await invoke("extension_add", { dir: ext.dir }).catch(() => null);
      toggle.disabled = false;
      if (!added) {
        note2.textContent = t("The browser refused that; restart and try again");
        return;
      }
      enabledDirs.add(ext.dir);
      toggle.setAttribute("aria-checked", "true");
      note2.textContent = t("Enabled");
    });
    actions.append(toggle);

    card.append(actions);
    grid.append(card);
  }
}

boot();