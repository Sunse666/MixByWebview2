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

      const toggle = document.createElement("button");
      toggle.className = "switch";
      toggle.setAttribute("role", "switch");
      toggle.setAttribute("aria-checked", String(Boolean(item.enabled)));
      toggle.addEventListener("click", async () => {
        const enable = toggle.getAttribute("aria-checked") !== "true";
        toggle.setAttribute("aria-checked", String(enable));
        const ok = await invoke("extension_live_set", { id: item.id, enabled: enable }).catch(
          () => false,
        );
        await invoke("ui_log", {
          message: "live toggle " + item.id + " -> " + enable + ": " + ok,
        }).catch(() => {});
        toggle.setAttribute("aria-checked", String(Boolean(ok) ? enable : !enable));
      });

      line.append(name, id, toggle);
      liveCard.append(line);
    }
    main.append(liveCard);
  }

  // Turning an extension off is durable in the browser's own profile, and it stops
  // showing in the live list above, so offer the way back instead of leaving it
  // hidden with no control.
  const resetNote = document.createElement("div");
  resetNote.className = "card";
  resetNote.style.maxWidth = "720px";
  resetNote.style.marginBottom = "18px";
  const resetText = document.createElement("span");
  resetText.className = "title";
  resetText.textContent = t("An extension turned off here does not come back on its own");
  const resetButton = document.createElement("button");
  resetButton.className = "button";
  resetButton.textContent = t("Bring disabled extensions back (needs a restart)");
  resetButton.addEventListener("click", async () => {
    const answer = await invoke("extension_reset").catch((error) => String(error));
    resetText.textContent =
      answer === "restart"
        ? t("Restart Mix to load them again")
        : t("Could not reset:") + " " + String(answer);
  });
  resetNote.append(resetText, resetButton);
  main.append(resetNote);

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
    const liveEntry = live.find((item) => item.name === ext.name);
    const toggle = document.createElement("button");
    toggle.className = "switch";
    toggle.setAttribute("role", "switch");
    toggle.setAttribute("aria-checked", String(Boolean(liveEntry && liveEntry.enabled)));
    toggle.title = t("Enabled");
    toggle.addEventListener("click", async () => {
      const on = toggle.getAttribute("aria-checked") === "true";
      toggle.disabled = true;
      const done = on
        ? await invoke("extension_live_set", { id: liveEntry.id, enabled: false }).catch(() => false)
        : await invoke("extension_add", { dir: ext.dir }).catch(() => false);
      toggle.disabled = false;
      if (!done) {
        note2.textContent = t("The browser refused that; restart and try again");
        return;
      }
      toggle.setAttribute("aria-checked", String(!on));
      note2.textContent = on ? t("Disabled") : t("Enabled");
    });
    actions.append(toggle);

    card.append(actions);
    grid.append(card);
  }
}

boot();