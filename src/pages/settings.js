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
  if (page) item.addEventListener("click", () => invoke("tab_navigate", { url: page }).catch(() => {}));
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

  const bar = document.createElement("div");
  bar.className = "card-actions";
  const theme = document.createElement("button");
  theme.className = "button";
  theme.textContent = t("Appearance & theme");
  theme.addEventListener("click", () =>
    invoke("overlay_open", { kind: "app_menu&page=settings" }).catch(() => {}),
  );
  bar.append(theme);
  card.append(bar);

  card.append(row("Language", "Choose the language of the interface", language));

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
