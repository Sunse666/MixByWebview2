import { setLanguage, t } from "../i18n.js";

const { invoke } = window.__TAURI__.core;

const nav = document.getElementById("nav");
const main = document.getElementById("main");

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

function row(title, detail, control) {
  const line = document.createElement("div");
  line.className = "entry";
  const text = document.createElement("span");
  text.className = "title";
  text.textContent = t(title);
  if (detail) text.title = detail;
  line.append(text);
  if (control) line.append(control);
  return line;
}

function themePreview() {
  const holder = document.createElement("div");
  holder.className = "card-actions";
  const edit = document.createElement("button");
  edit.className = "button";
  edit.textContent = t("Edit");
  edit.addEventListener("click", () => openThemeEditor(holder));
  holder.append(edit);
  return holder;
}

async function openThemeEditor(holder) {
  const theme = await invoke("theme_get").catch(() => null);
  if (!theme) return;

  holder.replaceChildren();

  const draft = {
    colors: { ...theme.colors },
    radius: theme.radius,
    tab_radius: theme.tab_radius,
    font_size: theme.font_size,
    font_family: theme.font_family,
  };

  const fields = [
    ["Background", "bg"],
    ["Chrome", "chrome"],
    ["Accent", "accent"],
    ["Text", "text"],
    ["Border", "border"],
    ["Tab", "tab"],
    ["URL field", "url_bg"],
  ];

  for (const [label, key] of fields) {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "hex";
    input.value = draft.colors[key] || "";
    input.placeholder = "#RRGGBB";
    input.addEventListener("input", () => {
      draft.colors[key] = input.value.trim();
    });
    holder.append(row(label, "", input));
  }

  for (const [label, key, min, max] of [
    ["Corner radius", "radius", 0, 20],
    ["Tab radius", "tab_radius", 0, 20],
    ["Font size", "font_size", 11, 20],
  ]) {
    const input = document.createElement("input");
    input.type = "number";
    input.className = "num";
    input.min = String(min);
    input.max = String(max);
    input.value = String(draft[key]);
    input.addEventListener("input", () => {
      draft[key] = Number(input.value);
    });
    holder.append(row(label, "", input));
  }

  const save = document.createElement("button");
  save.className = "button";
  save.textContent = t("Save");
  save.addEventListener("click", async () => {
    await invoke("theme_apply", { theme: draft, persist: true }).catch(() => {});
    holder.replaceChildren(save);
    holder.firstChild.remove();
    holder.append(editButton(holder));
  });

  const reset = document.createElement("button");
  reset.className = "button";
  reset.textContent = t("Cancel");
  reset.addEventListener("click", () => holder.replaceChildren(editButton(holder)));

  const actions = document.createElement("div");
  actions.className = "card-actions";
  actions.append(save, reset);
  holder.append(actions);

  // live preview while editing: applied but not written to theme.json yet
  const preview = () => invoke("theme_apply", { theme: draft, persist: false }).catch(() => {});
  holder.addEventListener("input", preview);
}

function editButton(holder) {
  const edit = document.createElement("button");
  edit.className = "button";
  edit.textContent = t("Edit");
  edit.addEventListener("click", () => openThemeEditor(holder));
  return edit;
}

function toggle(on, onChange) {
  const button = document.createElement("button");
  button.className = "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  button.addEventListener("click", async () => {
    const next = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(next));
    await onChange(next);
  });
  return button;
}

async function boot() {
  let settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  document.title = t("Settings");

  const pages = {
    history: "http://tauri.localhost/pages/history.html",
    extensions: "http://tauri.localhost/pages/extensions.html",
  };
  navItem("⟲", "History", false, pages.history);
  navItem("☆", "Bookmarks", false, null);
  navItem("⧉", "Extensions", false, pages.extensions);
  navItem("⚙", "Settings", true, null);

  const head = document.createElement("h2");
  head.textContent = t("Settings");
  main.append(head);

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  card.append(
    row(
      "Restore session",
      "Restore the last session on start",
      toggle(settings.restore_session, async (value) => {
        await invoke("settings_set", { patch: { restore_session: value } }).catch(() => {});
      }),
    ),
  );

  card.append(
    row(
      "Bookmarks bar",
      "Show the bookmarks bar",
      toggle(settings.show_bookmarks, async (value) => {
        await invoke("settings_set", { patch: { show_bookmarks: value } }).catch(() => {});
      }),
    ),
  );

  const language = document.createElement("select");
  language.className = "button";
  for (const [value, label] of [["en", "English"], ["zh", "Simplified Chinese"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.language || "en") === value) option.selected = true;
    language.append(option);
  }
  language.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { language: language.value } }).catch(() => {});
    setLanguage(language.value);
    location.reload();
  });
  card.append(
    row(
      "Menu bar",
      "Show the menu bar row",
      toggle(settings.menu_bar !== false, async (value) => {
        await invoke("settings_set", { patch: { menu_bar: value } }).catch(() => {});
      }),
    ),
  );

  card.append(row("Appearance & theme", "", themePreview()));

  const engine = document.createElement("select");
  engine.className = "button";
  for (const [value, label] of [
    ["baidu", "Baidu"],
    ["bing", "Bing"],
    ["google", "Google"],
    ["ddg", "DuckDuckGo"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    if ((settings.search_engine || "baidu") === value) option.selected = true;
    engine.append(option);
  }
  engine.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { search_engine: engine.value } }).catch(() => {});
  });
  card.append(row("Search engine", "Used when the address bar is not a url", engine));

  card.append(row("Language", "Choose the language of the interface", language));

  const dataHead = document.createElement("h2");
  dataHead.textContent = t("Clear browsing data");
  dataHead.style.margin = "18px 0 10px";
  main.append(dataHead);

  const dataCard = document.createElement("div");
  dataCard.className = "card";
  dataCard.style.maxWidth = "720px";

  const KINDS = {
    history: "History",
    bookmarks: "Bookmarks",
    favicons: "Site icons",
    zoom: "Zoom levels",
    session: "Session",
  };
  const picked = new Set(["history"]);

  for (const [kind, label] of Object.entries(KINDS)) {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "pick";
    box.checked = picked.has(kind);
    box.addEventListener("change", () => {
      if (box.checked) picked.add(kind);
      else picked.delete(kind);
    });
    const line = document.createElement("div");
    line.className = "entry";
    const text = document.createElement("span");
    text.className = "title";
    text.textContent = t(label);
    line.append(box, text);
    dataCard.append(line);
  }

  const run = document.createElement("button");
  run.className = "button";
  run.textContent = t("Clear selected data");
  run.addEventListener("click", async () => {
    const done = await invoke("clear_data", { kinds: [...picked] }).catch(() => []);
    run.textContent = t("Cleared") + ": " + (done || []).join(", ");
    setTimeout(() => {
      run.textContent = t("Clear selected data");
    }, 2500);
  });
  const runRow = document.createElement("div");
  runRow.className = "card-actions";
  runRow.append(run);
  dataCard.append(runRow);
  main.append(dataCard);

  const startHead = document.createElement("h2");
  startHead.textContent = t("Startup");
  startHead.style.margin = "18px 0 10px";
  main.append(startHead);

  const startCard = document.createElement("div");
  startCard.className = "card";
  startCard.style.maxWidth = "720px";

  const startUrl = document.createElement("input");
  startUrl.className = "hex";
  startUrl.style.width = "320px";
  startUrl.value = settings.start_url || "";
  startUrl.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { start_url: startUrl.value } }).catch(() => {});
  });
  startCard.append(row("Start page", "Opened at launch", startUrl));
  main.append(startCard);

  const browserHead = document.createElement("h2");
  browserHead.textContent = t("Default browser");
  browserHead.style.margin = "18px 0 10px";
  main.append(browserHead);

  const browserCard = document.createElement("div");
  browserCard.className = "card";
  browserCard.style.maxWidth = "720px";

  const register = document.createElement("button");
  register.className = "button";
  register.textContent = t("Register as a browser");
  const status = document.createElement("span");
  status.className = "card-note";
  register.addEventListener("click", async () => {
    const result = await invoke("register_browser").catch((error) => "error: " + String(error));
    status.textContent = String(result);
  });
  const row2 = document.createElement("div");
  row2.className = "card-actions";
  row2.append(register, status);
  browserCard.append(
    row(t("Default browser"), t("Adds Mix to the Windows default apps list")),
    row2,
  );
  main.append(browserCard);

  const toolbarHead = document.createElement("h2");
  toolbarHead.textContent = t("Toolbar buttons");
  toolbarHead.style.margin = "18px 0 10px";
  main.append(toolbarHead);

  const toolbarCard = document.createElement("div");
  toolbarCard.className = "card";
  toolbarCard.style.maxWidth = "720px";

  const NAMES = {
    downloads: "Downloads",
    translate: "Translate",
    zoom: "Zoom",
    find: "Find in page",
    bookmark: "Bookmark",
    extensions: "Extensions",
  };
  const enabled = new Set(settings.toolbar || []);
  for (const [id, label] of Object.entries(NAMES)) {
    toolbarCard.append(
      row(
        label,
        "",
        toggle(enabled.has(id), async (value) => {
          const next = new Set(settings.toolbar || []);
          if (value) next.add(id);
          else next.delete(id);
          settings.toolbar = [...next];
          await invoke("settings_set", { patch: { toolbar: [...next] } }).catch(() => {});
        }),
      ),
    );
  }


  main.append(card);
  main.append(toolbarHead, toolbarCard);
}

boot();
