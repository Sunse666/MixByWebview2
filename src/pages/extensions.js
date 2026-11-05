import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

function navItem(glyph, label, active) {
  const item = document.createElement("div");
  item.className = "nav-item" + (active ? " active" : "");
  const icon = document.createElement("span");
  icon.textContent = glyph;
  const text = document.createElement("span");
  text.textContent = t(label);
  item.append(icon, text);
  nav.append(item);
}

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");

  navItem("⟲", "History", false);
  navItem("☆", "Bookmarks", false);
  navItem("⧉", "Extensions", true);
  navItem("⚙", "Settings", false);

  const head = document.createElement("h2");
  head.textContent = t("Extensions");
  main.append(head);

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

    const toggle = document.createElement("button");
    toggle.className = "switch";
    toggle.setAttribute("role", "switch");
    toggle.setAttribute("aria-checked", String(!hidden.has(ext.name)));
    toggle.title = t("Show in the toolbar");
    toggle.addEventListener("click", async () => {
      const on = toggle.getAttribute("aria-checked") !== "true";
      toggle.setAttribute("aria-checked", String(on));
      if (on) hidden.delete(ext.name);
      else hidden.add(ext.name);
      await invoke("settings_set", {
        patch: { hidden_extensions: [...hidden] },
      }).catch(() => {});
    });
    actions.append(toggle);

    card.append(actions);
    grid.append(card);
  }
}

boot();
