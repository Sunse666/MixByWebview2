// The overlay page: menus, panels, permission prompts and the tab hover card.
const { invoke } = window.__TAURI__.core;

/// Leaving the overlay closes it: the pointer went somewhere else, or another window took
/// the keyboard.
///
/// Not on every blur, though. Resizing a child webview - which is what opening the history
/// column beside the menu does - takes the focus away from the page for a moment, and the
/// panel closed itself a heartbeat after it was asked to draw a second column. A blur that
/// the page causes on itself is not the pointer going anywhere.
let resizingUntil = 0;
window.__mixSizing = (hold) => {
  resizingUntil = Date.now() + (hold || 400);
};
window.addEventListener("blur", () => {
  if (Date.now() < resizingUntil) return;
  invoke("overlay_close").catch(() => {});
});
import { applyStatic, setLanguage, t } from "./i18n.js";
const { listen } = window.__TAURI__.event;

const root = document.documentElement;
const fields = {
  bg: document.getElementById("bg"),
  chrome: document.getElementById("chrome"),
  tab: document.getElementById("tab"),
  tab_text: document.getElementById("tab-text"),
  text: document.getElementById("text"),
  accent: document.getElementById("accent"),
  url_bg: document.getElementById("url-bg"),
  hover: document.getElementById("hover"),
  border: document.getElementById("border"),
};
const radius = document.getElementById("radius");
const radiusOut = document.getElementById("radius-out");
const tabRadius = document.getElementById("tab-radius");
const tabRadiusOut = document.getElementById("tab-radius-out");
const fontSize = document.getElementById("font-size");
const fontOut = document.getElementById("font-out");
const fontFamily = document.getElementById("font-family");

const PRESETS = {
  green: { bg: "#1F2020", chrome: "#1F2020", accent: "#00FF00", text: "#E8E8E8", border: "#2E3131", radius: 15 },
  dark: { bg: "#111315", chrome: "#181B1E", accent: "#8AB4F8", text: "#DADCE0", border: "#2A2F34", radius: 8 },
  light: { bg: "#F5F6F7", chrome: "#FFFFFF", accent: "#1A73E8", text: "#202124", border: "#D6D9DC", radius: 10 },
};

const report = (message) => invoke("ui_log", { message }).catch(() => {});
report(`overlay boot hash=${location.hash} size=${window.innerWidth}x${window.innerHeight}`);

const rawHash = location.hash.replace("#", "") || "settings";
const kind = rawHash.split("&")[0];
const params = new Map(
  rawHash
    .split("&")
    .slice(1)
    .map((pair) => pair.split("="))
    .map(([key, value]) => [key, decodeURIComponent(value || "")]),
);
let current = null;

function applyTheme(theme) {
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

function render(theme) {
  applyTheme(theme);
  for (const [key, input] of Object.entries(fields)) input.value = theme.colors[key];
  radius.value = String(Math.round(theme.radius));
  radiusOut.textContent = `${Math.round(theme.radius)}px`;
  tabRadius.value = String(Math.round(theme.tab_radius));
  tabRadiusOut.textContent = `${Math.round(theme.tab_radius)}px`;
  fontSize.value = String(Math.round(theme.font_size));
  fontOut.textContent = `${Math.round(theme.font_size)}px`;
  fontFamily.value = theme.font_family;
}

async function preview() {
  const colors = { ...current.colors };
  for (const [key, input] of Object.entries(fields)) colors[key] = input.value;
  const next = {
    ...current,
    colors,
    radius: Number(radius.value),
    tab_radius: Number(tabRadius.value),
    font_size: Number(fontSize.value),
    font_family: fontFamily.value,
  };
  current = await invoke("theme_apply", { theme: next, persist: false });
  radiusOut.textContent = `${Math.round(current.radius)}px`;
  tabRadiusOut.textContent = `${Math.round(current.tab_radius)}px`;
  fontOut.textContent = `${Math.round(current.font_size)}px`;
}

async function close() {
  await invoke("overlay_close");
}

const menu = document.getElementById("menu");
const appMenu = document.getElementById("app-menu");
const themePanel = document.getElementById("theme-panel");
const permissionPanel = document.getElementById("permission-panel");
const autostartBox = document.getElementById("autostart");
const restoreBox = document.getElementById("restore");

async function bootAppMenu() {
  appMenu.hidden = false;
  themePanel.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const close = () => invoke("overlay_close");
  const info = await invoke("app_info");

  const clear = () => {
    while (appMenu.firstChild) appMenu.removeChild(appMenu.firstChild);
  };

  const divider = () => appMenu.append(Object.assign(document.createElement("div"), { className: "divider" }));

  const header = (title) => {
    const bar = document.createElement("div");
    bar.className = "menu-header";
    const back = document.createElement("button");
    back.className = "back";
    back.textContent = "\u2039";
    back.title = t("Back");
    back.addEventListener("click", () => render("root"));
    const label = document.createElement("span");
    label.textContent = t(title);
    bar.append(back, label);
    appMenu.append(bar);
  };

  const row = (glyph, label, hint, run, options = {}) => {
    const button = document.createElement("button");
    button.className = "row" + (options.danger ? " danger" : "") + (options.muted ? " muted" : "");

    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;

    const text = document.createElement("span");
    text.className = "label";
    text.textContent = t(label);

    const key = document.createElement("span");
    key.className = "hint";
    key.textContent = hint || (options.page ? "\u203a" : "");

    button.append(icon, text, key);
    if (run) button.addEventListener("click", run);
    appMenu.append(button);
    return button;
  };

  const act = (fn) => async () => {
    await fn();
    await close();
  };

  let NEXT_SUB = 0;

  // A submenu, drawn beside the menu rather than in a webview of its own.
  //
  // A child webview cannot be placed next to its parent, and opening a second overlay
  // closes the first, so the only way two menus can stand side by side is for one panel to
  // draw both. That panel is opened wide enough for both from the start - see `MENU_W`,
  // `GAP_W` and `SUBMENU_W` in overlay.rs - so the column appears in room that is already
  // there and nothing about the panel's width changes when it does.
  /// Never shorter than this, even when the panel is nearly out of room.
  const MIN_ROW = 48;
  let openSub = null;
  /// The row whose column is on screen, or null. Declared next to `openSub` because the
  /// two describe the same open column and are set and cleared together.
  let openHost = null;

  /// The height the panel had before the column opened, so closing returns to that one.
  ///
  /// Deliberately not re-measured on the way out. Two numbers describe this menu and only
  /// one of them is a height the panel can have: the content is taller than the window, so
  /// `menuContentHeight()` answers ~835 while `overlay_resize` clamps whatever it is handed
  /// to the room there is (~684). Asking for the content height on the way out sized the
  /// menu to the clamp - taller than it opened - and it never came back down, because the
  /// next open/close pair read that same inflated number again.
  let openedHeight = 0;

  /// Takes the column away.
  ///
  /// The width never changes: the panel was opened wide enough for both boxes and the
  /// column is drawn into room that was already there, so there is no resize on the way out
  /// and therefore no `narrow` flag. Only the height goes back, because the panel had to be
  /// as tall as the window while a full-height column was standing in it.
  const closeSub = () => {
    if (!openSub) return;
    openSub.remove();
    openSub = null;
    openHost = null;
    document.body.classList.remove("has-sub");
    // Only the row whose column this was loses the lit state; a row that opens a different
    // column has already been lit by the call that got here.
    for (const lit of appMenu.querySelectorAll(".row.open")) lit.classList.remove("open");
    // Back to whatever `.menu` says: the pinned height was for the open column only, and
    // leaving it would freeze the menu at today's size - a later build with one more row
    // would keep the pin and scroll forever.
    appMenu.style.height = "";
    // Held as long as the previous resize takes to land, then released: this one takes the
    // focus away as well, and the blur that follows is the page's own doing.
    window.__mixSizing(400);
    invoke("overlay_resize", { height: openedHeight || 566 }).catch(() => {});
  };

  // A column beside the menu is drawn into room the panel already had, so nothing here
  // resizes anything. What the menu does need is one height: the column runs the full
  // height of the window, and the webview clips whatever is below its box, so the panel has
  // to be as tall as the window while the column is up. Asking for more than any window has
  // is how that number is obtained - `overlay_resize` clamps it to the room there is, which
  // is the one number the page cannot know.
  const openColumnHeight = () => {
    // Held open: the resize takes the focus away from the page for a moment, and the blur
    // handler would read that as the pointer leaving and close the column it just opened.
    window.__mixSizing(60000);
    invoke("overlay_resize", { height: 100000 }).catch(() => {});
  };

  const showSub = (host, build) => {
    if (openSub && openSub.dataset.for === host.dataset.sub) return;
    // Only on the way in: replacing one column with another must not overwrite the height
    // being remembered with the height the open column forced on the panel.
    if (!openSub) {
      openedHeight = Math.ceil(appMenu.getBoundingClientRect().height);
      // Pinned in pixels, because every other unit here follows the viewport and the
      // viewport is about to grow. `.has-sub` sets `height: auto; max-height: 100%`, and
      // for a fixed box `100%` is the webview - which is at this moment being enlarged to
      // the whole window. The menu would then size itself to the room it was just given
      // (684) instead of the height it opened at (567), and the rows would sit in a box
      // taller than themselves. The height the panel had is the height it keeps.
      appMenu.style.height = `${openedHeight}px`;
    }
    closeSub();
    host.classList.add("open");
    openHost = host;
    const panel = document.createElement("div");
    // `menu` as well as `submenu`: every row style in the stylesheet is written against
    // `.menu .row`, so a column without that class draws its rows as bare buttons -
    // boxed, bordered and centred, which is what the first attempt looked like.
    panel.className = "menu submenu";
    panel.dataset.for = host.dataset.sub;
    document.body.append(panel);
    document.body.classList.add("has-sub");
    openSub = panel;
    placeSub();
    // A list is read from the history, which is a command: the panel is on screen with a
    // placeholder first and filled when the answer arrives, so hovering feels immediate.
    if (build) Promise.resolve(build(panel)).then(() => {
      if (openSub !== panel) return;
      // Sized again now that the rows exist: the panel follows the taller of the two.
      placeSub();
    });
  };

  /// How tall the menu column needs to be, in its own right.
  ///
  /// Measured from the rows and the padding rather than from `scrollHeight`: the box is
  /// already clamped to the panel that this number is going to size, so reading the box
  /// back gives the height the panel already has and the two can only ever agree.
  const menuContentHeight = () => {
    const style = getComputedStyle(appMenu);
    const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    let rows = 0;
    for (const child of appMenu.children) rows += child.offsetHeight;
    return rows + padding;
  };

  /// The submenu is a full-height column beside the menu, not a bubble beside a row.
  ///
  /// This is the shape a browser uses for a list that comes out of the app menu: the panel
  /// grows to the whole height it is allowed and the column fills it from the top of the
  /// window to the bottom. A box anchored to the row was tried and is what every earlier
  /// attempt looked wrong about - it floats a few hundred pixels into the middle of the
  /// window, attached to nothing, and the further down the menu its row sits the lower it
  /// hangs and the less of it there is to see.
  const placeSub = () => {
    if (!openSub) return;
    openSub.style.top = "0";
    openSub.style.maxHeight = "";
    openColumnHeight();
  };

  /// A row that opens `build` beside it while the pointer is on it.
  const rowMenu = (glyph, label, hint, build) => {
    const button = row(glyph, label, hint || "\u203a", null);
    button.dataset.sub = String(NEXT_SUB++);
    button.addEventListener("pointerenter", () => showSub(button, build));
    button.addEventListener("click", () => showSub(button, build));
    return button;
  };

  /// The site behind a url, or the url itself when it does not parse.
  function hostOf(url) {
    try {
      return new URL(url).host || url;
    } catch (error) {
      return url;
    }
  }

  /// The letter a row shows when a site has no icon to show.
  function initialOf(url) {
    const host = hostOf(url).replace(/^www\./, "");
    return (host[0] || "?").toUpperCase();
  }

  /// A column beside the menu: a heading that stays put, a list that scrolls, and the
  /// actions pinned to the bottom.
  ///
  /// Every one of these panels has that shape - history, downloads, bookmarks, extensions -
  /// and the three parts are what make it read as a panel rather than a stack of rows: the
  /// heading says which list this is while the middle scrolls under it, and the actions stay
  /// where a reader expects them however long the list gets.
  const column = (panel, title, key) => {
    const head = document.createElement("div");
    head.className = "sub-head";
    const name = document.createElement("span");
    name.className = "sub-title";
    name.textContent = t(title);
    head.append(name);
    if (key) {
      const hint = document.createElement("span");
      hint.className = "hint";
      hint.textContent = key;
      head.append(hint);
    }
    panel.append(head);
    return head;
  };

  /// The section label under the heading, e.g. "Recently opened".
  const section = (panel, label) => {
    const bar = document.createElement("div");
    bar.className = "sub-section";
    bar.textContent = t(label);
    panel.append(bar);
  };

  /// The scrolling part of a column.
  const listArea = (panel) => {
    const list = document.createElement("div");
    list.className = "sub-list";
    panel.append(list);
    return list;
  };

  /// The pinned part of a column.
  const footArea = (panel) => {
    const foot = document.createElement("div");
    foot.className = "sub-foot";
    panel.append(foot);
    return foot;
  };

  /// One line of a column, with the icon every browser puts in front of a page's name.
  ///
  /// The icon falls back to the first letter of the host: a site whose icon cannot be
  /// fetched would otherwise leave the row with a blank square where the others have a
  /// picture, which reads as a broken list rather than a missing picture.
  const entryRow = (list, { icon, label, hint, title, onPick }) => {
    const entry = document.createElement("button");
    entry.className = list.dataset.rows === "history" ? "row history-row" : "row";
    if (title) entry.title = title;

    const mark = document.createElement("span");
    mark.className = "ico";
    if (icon) {
      const img = document.createElement("img");
      img.className = "fav";
      img.src = icon;
      img.alt = "";
      img.addEventListener("error", () => {
        mark.replaceChildren(document.createTextNode(labelLetter(label)));
      });
      mark.append(img);
    } else {
      mark.textContent = labelLetter(label);
    }

    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label;

    entry.append(mark, text);
    if (hint) {
      const side = document.createElement("span");
      side.className = "hint";
      side.textContent = hint;
      entry.append(side);
    }
    if (onPick) entry.addEventListener("click", onPick);
    list.append(entry);
    return entry;
  };

  /// The letter to stand in for an icon when there is none.
  function labelLetter(text) {
    const clean = String(text || "").trim();
    return (clean[0] || "?").toUpperCase();
  }

  /// A note in place of a list that has nothing in it.
  const emptyNote = (list, text) => {
    const note = document.createElement("div");
    note.className = "menu-note";
    note.textContent = t(text);
    list.append(note);
  };

  /// url -> icon for a set of urls, from the host map the browser keeps.
  ///
  /// A bookmark and a download carry only an address, so the icon has to be looked up by
  /// host; the map is keyed the way `favicons` stores it, without `www.` and in lower case.
  const iconMapFor = async (urls) => {
    const out = new Map();
    if (!urls.length) return out;
    const icons = (await invoke("favicon_map").catch(() => ({}))) || {};
    for (const url of urls) {
      const host = hostOf(url).replace(/^www\./, "").toLowerCase();
      if (icons[host]) out.set(url, icons[host]);
    }
    return out;
  };

  /// An action pinned to the bottom of a column.
  const footRow = (foot, label, onPick) => {
    const button = document.createElement("button");
    button.className = "row plain sub-more";
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = t(label);
    button.append(text);
    button.addEventListener("click", onPick);
    foot.append(button);
    return button;
  };

  async function buildHistorySub(panel) {
    column(panel, "History", "Ctrl+H");
    section(panel, "Recently opened");
    const list = listArea(panel);
    list.dataset.rows = "history";
    const foot = footArea(panel);

    // As many as the panel can show, and it scrolls for the rest. The cap was six, from
    // when the column was only as tall as its rows: a short list then meant a short panel,
    // and anything longer was cut off rather than scrolled. The column fills the window
    // now, so the number is what the history command returns and the rest is a scroll away.
    const visits = (await invoke("history_list", { limit: 40 }).catch(() => [])) || [];
    const seen = new Set();
    const unique = visits.filter((entry) => {
      if (seen.has(entry.url)) return false;
      seen.add(entry.url);
      return true;
    });

    if (!unique.length) emptyNote(list, "No history yet.");
    // A row is an icon, the page's title and the site it came from - the shape every
    // browser uses for this list, and the one the history page itself already draws. A
    // title alone leaves the reader guessing at which of ten pages called "Video" this
    // is, which is the whole reason the icon is there.
    for (const visit of unique) {
      entryRow(list, {
        icon: visit.icon,
        label: visit.title || visit.url,
        hint: hostOf(visit.url),
        title: visit.url,
        onPick: async () => {
          await invoke("tab_navigate", { url: visit.url }).catch(() => {});
          await close();
        },
      });
    }
    // One action, at the bottom, where a browser puts it. The command existed before this
    // panel did and nothing called it: clearing history meant opening the page it is trying
    // to save you a visit to.
    footRow(foot, "Clear history", async () => {
      await invoke("history_clear").catch((error) => report(String(error)));
      await close();
    });
  }

  /// The pages this browser has been told about: what the reader bookmarked.
  async function buildBookmarksSub(panel) {
    column(panel, "Bookmarks", "Ctrl+Shift+O");
    const list = listArea(panel);
    list.dataset.rows = "history";
    const foot = footArea(panel);

    const marks = (await invoke("bookmark_list").catch(() => [])) || [];
    // A bookmark carries no icon of its own - the store maps hosts to icons elsewhere - so
    // the row falls back to the letter of the site, which is what the bookmarks page shows
    // when its icon lookup misses as well.
    const marksWithIcons = await iconMapFor(marks.map((mark) => mark.url));
    // Folders first, then the loose bookmarks: the order the bookmarks page uses, so the two
    // views of the same data do not disagree.
    const byFolder = new Map();
    for (const mark of marks) {
      const folder = mark.folder || "";
      if (!byFolder.has(folder)) byFolder.set(folder, []);
      byFolder.get(folder).push(mark);
    }
    const folders = [...byFolder.keys()].filter(Boolean).sort();

    if (!marks.length) {
      emptyNote(list, "No bookmarks yet. Press Ctrl+D on a page you want to keep.");
    } else {
      const emit = (mark) => {
        entryRow(list, {
          icon: marksWithIcons.get(mark.url),
          label: mark.title || mark.url,
          hint: hostOf(mark.url),
          title: mark.url,
          onPick: async () => {
            await invoke("tab_navigate", { url: mark.url }).catch(() => {});
            await close();
          },
        });
      };
      for (const folder of folders) {
        const bar = document.createElement("div");
        bar.className = "sub-section";
        bar.textContent = folder;
        list.append(bar);
        byFolder.get(folder).forEach(emit);
      }
      const loose = byFolder.get("") || [];
      if (loose.length) {
        if (folders.length) {
          const bar = document.createElement("div");
          bar.className = "sub-section";
          bar.textContent = t("Other bookmarks");
          list.append(bar);
        }
        loose.forEach(emit);
      }
    }

    footRow(foot, "Bookmark manager", () =>
      invoke("tab_new", { url: "http://tauri.localhost/pages/bookmarks.html" }),
    );
  }

  /// What this browser has been downloading, newest last, and the folder they land in.
  async function buildDownloadsSub(panel) {
    column(panel, "Downloads", "Ctrl+J");
    const list = listArea(panel);
    list.dataset.rows = "history";
    const foot = footArea(panel);

    const tasks = (await invoke("download_list").catch(() => [])) || [];
    if (!tasks.length) {
      emptyNote(list, "Nothing downloaded yet.");
    }
    // Newest first, which is the order a reader wants here - the list on the downloads page
    // is a log and reads downwards, but this column is shown for the thing just finished.
    const newest = [...tasks].reverse();
    const icons = await iconMapFor(
      newest.filter((task) => task.state === "finished").map((task) => task.url),
    );
    for (const task of newest) {
      const done = task.state === "finished";
      // `total` is what the server announced; a transfer that never got a length shows the
      // bytes written and no percentage, which is the honest answer.
      const got = megabytes(task.bytes || 0);
      const size = task.total ? `${got} of ${megabytes(task.total)}` : got;
      entryRow(list, {
        icon: done ? icons.get(task.url) || null : null,
        label: task.file_name || task.url,
        hint: task.state === "started" ? size : done ? size : t(task.state),
        title: task.path || task.url,
        onPick: done ? () => invoke("download_open", { id: task.id }).catch(() => {}) : null,
      });
    }

    footRow(foot, "Open downloads folder", () => invoke("open_dir", { which: "downloads" }));
    footRow(foot, "All downloads", () =>
      invoke("tab_new", { url: "http://tauri.localhost/pages/downloads.html" }),
    );
  }

  /// What the browser actually loaded, with the switch that keeps it off the toolbar.
  async function buildExtensionsSub(panel) {
    column(panel, "Extensions");
    const list = listArea(panel);
    list.dataset.rows = "history";
    const foot = footArea(panel);

    const settings = await invoke("settings_get").catch(() => ({}));
    const hidden = new Set((settings && settings.hidden_extensions) || []);
    const items = (await invoke("ext_list").catch(() => [])) || [];
    const loaded = items.filter((ext) => ext.status === "loaded");

    if (!loaded.length) {
      emptyNote(list, "No extensions loaded. Drop an MV3 extension into crx/ and restart.");
    }
    for (const ext of loaded) {
      const shown = !hidden.has(ext.name);
      entryRow(list, {
        icon: ext.icon || null,
        label: ext.version ? `${ext.name} ${ext.version}` : ext.name,
        hint: shown ? "\u2713" : "",
        title: ext.description || ext.name,
        // The row is the switch: hiding an extension here is the same write the extensions
        // page makes, and the column redraws so the tick follows the pointer's click.
        onPick: async () => {
          const next = new Set(hidden);
          if (next.has(ext.name)) next.delete(ext.name);
          else next.add(ext.name);
          await invoke("settings_set", { patch: { hidden_extensions: [...next] } });
          const again = [...appMenu.querySelectorAll(".row")].find(
            (row) => row.dataset.sub !== undefined && row.dataset.sub === openSub?.dataset.for,
          );
          if (again) again.dispatchEvent(new PointerEvent("pointerenter"));
        },
      });
    }

    footRow(foot, "Open extension folder", () => invoke("open_dir", { which: "extensions" }));
    footRow(foot, "Extensions page", () =>
      invoke("tab_new", { url: "http://tauri.localhost/pages/extensions.html" }),
    );
  }

  /// The corners of the app that are not browsing: a debugger, the log, the profile.
  async function buildToolsSub(panel) {
    column(panel, "More tools");
    const list = listArea(panel);
    const foot = footArea(panel);

    const add = (glyph, label, hint, run) => {
      const button = document.createElement("button");
      button.className = "row";
      const icon = document.createElement("span");
      icon.className = "ico";
      icon.textContent = glyph;
      const text = document.createElement("span");
      text.className = "label";
      text.textContent = t(label);
      const side = document.createElement("span");
      side.className = "hint";
      side.textContent = hint || "";
      button.append(icon, text, side);
      button.addEventListener("click", run);
      list.append(button);
    };

    add("\u2318", "Developer tools", "F12", () => invoke("shortcut", { action: "devtools" }));
    add("\u2691", "Export diagnostics", "", async () => {
      const path = await invoke("diag_export");
      report("diagnostics saved to " + path);
      await invoke("open_dir", { which: "diag" });
      await close();
    });
    add("\u26a0", "Open log file", "", () => invoke("open_dir", { which: "logs" }));
    add("\u2261", "Profile folder", "", () => invoke("open_dir", { which: "userdata" }));
    add("\u2699", "Diagnose super drag", "", () => invoke("drag_probe"));
    add("\u25a4", "Userscripts", "", () =>
      invoke("tab_new", { url: "http://tauri.localhost/pages/scripts.html" }),
    );

    footRow(foot, "Settings", () =>
      invoke("tab_new", { url: "http://tauri.localhost/pages/settings.html" }),
    );
  }

  /// What this build is, and where to look when it misbehaves.
  async function buildHelpSub(panel) {
    column(panel, "Help");
    const list = listArea(panel);
    const foot = footArea(panel);

    const note = document.createElement("div");
    note.className = "menu-note";
    note.textContent =
      `Mix ${info.version} (${info.git})\n` +
      "A Chromium shell over WebView2. Data lives in UserData next to the binary.";
    list.append(note);

    // The addresses the reader is most likely to want, each one a page this app serves.
    const add = (glyph, label, url) => {
      const button = document.createElement("button");
      button.className = "row";
      const icon = document.createElement("span");
      icon.className = "ico";
      icon.textContent = glyph;
      const text = document.createElement("span");
      text.className = "label";
      text.textContent = t(label);
      button.append(icon, text);
      button.addEventListener("click", () => invoke("tab_new", { url }));
      list.append(button);
    };
    add("\u2261", "Keyboard shortcuts", "http://tauri.localhost/pages/settings.html#shortcuts");
    add("\u2691", "Export diagnostics", "http://tauri.localhost/pages/settings.html#backup");

    footRow(foot, "Open log file", () => invoke("open_dir", { which: "logs" }));
  }

  const pages = {
    root: async () => {
      row("+", "New tab", "Ctrl+T", act(() => invoke("tab_new", {})));
      row("\u25a2", "New window", "Ctrl+N", act(() => invoke("window_new")));
      row("\u2315", "Find in page", "Ctrl+F", () => invoke("overlay_toggle", { kind: "find" }));
      row("\u25d1", "New incognito tab", "Ctrl+Shift+N", act(() => invoke("tab_new", { incognito: true })));
      row("\u21ba", "Reopen closed tab", "Ctrl+Shift+T", async () => {
        await invoke("tab_reopen", {});
        await close();
      });
      // The list behind that shortcut: twenty entries are kept per window, and only the
      // newest one used to be reachable.
      const closed = (await invoke("tab_closed_list").catch(() => [])) || [];
      row("\u2261", "Recently closed", closed.length ? String(closed.length) : "", () => render("closed"));
      rowMenu("\u2193", "Downloads", "Ctrl+J", (panel) => buildDownloadsSub(panel));
      divider();

      const zoomRow = document.createElement("div");
      zoomRow.className = "row zoom-row";
      const zoomIcon = document.createElement("span");
      zoomIcon.className = "ico";
      zoomIcon.textContent = "\u2315";
      const minus = document.createElement("button");
      minus.className = "step";
      minus.textContent = "\u2212";
      const level = document.createElement("button");
      level.className = "level";
      level.title = "Reset zoom";
      const plus = document.createElement("button");
      plus.className = "step";
      plus.textContent = "+";
      let zoom = await invoke("zoom_get");
      const paint = (value) => {
        zoom = value;
        level.textContent = Math.round(value * 100) + "%";
      };
      paint(zoom);
      minus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: -0.1 })));
      plus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 0.1 })));
      level.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 1 - zoom })));
      const controls = document.createElement("div");
      controls.className = "zoom-controls";
      controls.append(minus, level, plus);
      zoomRow.append(zoomIcon, controls);
      appMenu.append(zoomRow);

      row("\u25a3", "Full screen", "F11", act(() => invoke("window_fullscreen", { enabled: null })));
      divider();
      row("\u2399", "Print", "Ctrl+P", act(() => invoke("tab_print")));
      // Translates the page there and then. The row used to open a "translate" panel that
      // does not exist, so it fell through to the settings panel and showed the theme editor
      // instead; one command does the work either way.
      const translated = await invoke("translate_state").catch(() => false);
      row("\u4e16", translated ? "Show the original" : "Translate this page", "", async () => {
        await invoke("translate_toggle").catch(() => {});
        await close();
      });
      // Kept out of the settings page: a site is only ever "this one", and the page in front
      // of the user is what decides which.
      const always = await invoke("translate_site_state").catch(() => false);
      row("\u25c9", always ? "Stop translating this site" : "Always translate this site", always ? "\u2713" : "", async () => {
        await invoke("translate_site", { enable: !always }).catch(() => {});
        await render("root");
        await close();
      });
      row("\u21aa", "Open in the full translator", "", async () => {
        await invoke("translate_page", { target: null }).catch((error) => report(String(error)));
        await close();
      });
      row("\u2717", "Clear browsing data", "Ctrl+Shift+Del", async () => {
        await invoke("app_clear_data");
        await close();
      });
      divider();
      row("\u2606", "Bookmark this page", "Ctrl+D", async () => {
        const added = await invoke("bookmark_toggle");
        report("bookmark toggled -> " + added);
        await close();
      });
      rowMenu("\u2606", "Bookmarks", "", (panel) => buildBookmarksSub(panel));
      // The visits open beside the row while the pointer is on it: reading the last few
      // should not cost a click, and the page is still at the bottom of the list for the
      // search and the per-site filter it carries.
      rowMenu("\u21ba", "History", "", (panel) => buildHistorySub(panel));
      divider();
      rowMenu("\u25a4", "Extensions", "", (panel) => buildExtensionsSub(panel));
      rowMenu("\u2261", "More tools", "", (panel) => buildToolsSub(panel));
      divider();
      row("\u2699", "Settings", "", act(() => invoke("tab_new", { url: "http://tauri.localhost/pages/settings.html" })));
      rowMenu("\u003f", "Help", "", (panel) => buildHelpSub(panel));

      const about = document.createElement("div");
      about.className = "about";
      about.textContent = "Mix " + info.version + " (" + info.git + ")";
      appMenu.append(about);

      row("\u23fb", "Quit", "Ctrl+Q", () => invoke("app_quit"), { danger: true });
    },

    scripts: async () => {
      header("Userscripts");
      const list = await invoke("script_list");
      if (!list.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No userscripts loaded. Drop *.user.js files into javascript/ and restart.";
        appMenu.append(empty);
      }
      for (const script of list) {
        const item = document.createElement("div");
        item.className = "menu-entry";
        const name = document.createElement("div");
        name.className = "entry-name";
        name.textContent = script.name;
        const state = document.createElement("div");
        state.className = "entry-note";
        state.textContent = t(script.run_at) + " \u00b7 " + script.matches.join(", ");
        item.append(name, state);
        appMenu.append(item);
      }
      divider();
      row("\u2699", "Open userscripts folder", "", act(() => invoke("open_dir", { which: "scripts" })));
    },

    // The toolbar's extensions button opens this. It is a page of its own because the
    // button sits on the strip, not in the menu, and the two lists it needs - what the
    // browser loaded and what the user chose to hide - belong together.
    extensions: async () => {
      header("Extensions");
      const settings = await invoke("settings_get");
      const list = await invoke("ext_list").catch(() => []);
      const loaded = (list || []).filter((ext) => ext.status === "loaded");
      const hidden = new Set(settings.hidden_extensions || []);

      if (!loaded.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = "No extensions loaded. Drop an MV3 extension into crx/ and restart.";
        appMenu.append(empty);
      }

      for (const ext of loaded) {
        const shown = !hidden.has(ext.name);
        const item = document.createElement("button");
        item.className = "row";
        const icon = document.createElement("span");
        icon.className = "ico";
        icon.textContent = shown ? "\u2611" : "\u2610";
        const label = document.createElement("span");
        label.className = "label";
        label.textContent = ext.version ? `${ext.name} ${ext.version}` : ext.name;
        const hint = document.createElement("span");
        hint.className = "hint";
        hint.textContent = shown ? "" : t("Hidden");
        item.append(icon, label, hint);
        item.addEventListener("click", async () => {
          const next = new Set(hidden);
          if (next.has(ext.name)) next.delete(ext.name);
          else next.add(ext.name);
          await invoke("settings_set", { patch: { hidden_extensions: [...next] } });
          await render("extensions");
        });
        appMenu.append(item);
      }

      divider();
      row("\u25a4", "Open extension folder", "", () =>
        invoke("open_dir", { which: "extensions" }), { page: true });
      row("\u2699", "Extensions page", "", act(() =>
        invoke("tab_new", { url: "http://tauri.localhost/pages/extensions.html" })));
    },

    closed: async () => {
      header("Recently closed");
      const entries = (await invoke("tab_closed_list").catch(() => [])) || [];
      if (!entries.length) {
        const empty = document.createElement("div");
        empty.className = "menu-note";
        empty.textContent = t("Nothing closed yet.");
        appMenu.append(empty);
        return;
      }
      entries.forEach((entry, index) => {
        const label = entry.title || entry.url.replace(/^https?:\/\//, "");
        const short = label.length > 58 ? label.slice(0, 57) + "\u2026" : label;
        const button = row("\u21ba", short, index === 0 ? "Ctrl+Shift+T" : "", async () => {
          await invoke("tab_reopen", { index });
          await close();
        });
        button.title = entry.url;
      });
    },

    tools: async () => {
      header("More tools");
      row("\u2318", "Developer tools", "F12", act(() => invoke("shortcut", { action: "devtools" })));      row("\u26a0", "Open log file", "", act(() => invoke("open_dir", { which: "logs" })));
      row("\u2261", "Profile folder", "", act(() => invoke("open_dir", { which: "userdata" })));
      row("\u2691", "Export diagnostics", "", async () => {
        const path = await invoke("diag_export");
        report("diagnostics saved to " + path);
        await invoke("open_dir", { which: "diag" });
        await close();
      });
    },

    language: async () => {
      header("Language");
      const settings = await invoke("settings_get");
      const current = settings.language || "en";
      const choose = (value) => async () => {
        await invoke("settings_set", { patch: { language: value } });
        await applyLanguage();
        render("language");
      };
      row(current === "en" ? "\u25c9" : "\u25cb", "English", "", choose("en"));
      row(current === "zh" ? "\u25c9" : "\u25cb", "Simplified Chinese", "", choose("zh"));
    },

    toolbar: async () => {
      header("Toolbar");
      const settings = await invoke("settings_get");
      const enabled = new Set(settings.toolbar || []);
      const names = {
        downloads: "Downloads",
        translate: "Translate",
        find: "Find in page",
        bookmark: "Bookmark",
        extensions: "Extensions",
      };

      for (const id of Object.keys(names)) {
        const on = enabled.has(id);
        row(on ? "\u2611" : "\u2610", names[id], "", async () => {
          const next = new Set(settings.toolbar || []);
          if (next.has(id)) next.delete(id);
          else next.add(id);
          await invoke("settings_set", { patch: { toolbar: [...next] } });
          render("toolbar");
        });
      }

      divider();
      const loaded = (await invoke("ext_list")).filter((ext) => ext.status === "loaded");
      if (!loaded.length) {
        const note = document.createElement("div");
        note.className = "menu-note";
        note.textContent = "No extensions loaded.";
        appMenu.append(note);
        return;
      }
      const hidden = new Set(settings.hidden_extensions || []);
      for (const ext of loaded) {
        const shown = !hidden.has(ext.name);
        row(shown ? "\u2611" : "\u2610", ext.name, "", async () => {
          const next = new Set(settings.hidden_extensions || []);
          if (next.has(ext.name)) next.delete(ext.name);
          else next.add(ext.name);
          await invoke("settings_set", { patch: { hidden_extensions: [...next] } });
          render("toolbar");
        });
      }
    },

    settings: async () => {
      header("Appearance");
      row("\u25d0", "Appearance", "", () =>
        invoke("overlay_open", { kind: "settings&section=appearance" }), { page: true });
      row("\u25b6", "Startup", "", () =>
        invoke("overlay_open", { kind: "settings&section=startup" }), { page: true });
      act(() => {});
      row("\u1f310", "Language", "", () => render("language"), { page: true });
      row("\u2398", "Theme file", "", () =>
        invoke("overlay_open", { kind: "settings&section=theme" }), { page: true });
      divider();
      row("\u25a4", "Extensions", "", act(() => invoke("tab_new", { url: "http://tauri.localhost/pages/extensions.html" })));
      row("\u2699", "Userscripts", "", () => render("scripts"), { page: true });
      divider();
      row("\u2261", "Open profile folder", "", act(() => invoke("open_dir", { which: "userdata" })));
    },

    help: async () => {
      header("Help");
      row("\u2318", "Keyboard shortcuts", "", () => render("shortcuts"), { page: true });
      row("\u25a4", "Open program folder", "", act(() => invoke("open_dir", { which: "root" })));
      divider();
      const about = document.createElement("div");
      about.className = "menu-note";
      about.textContent =
        "Mix " + info.version + " (" + info.git + ")\n" +
        info.os + " \u00b7 WebView2 " + info.webview;
      appMenu.append(about);
    },

    shortcuts: async () => {
      header("Keyboard shortcuts");
      const keys = [
        ["Ctrl+T", "New tab"],
        ["Ctrl+N", "New window"],
        ["Ctrl+Shift+N", "New incognito tab"],
        ["Ctrl+Shift+T", "Reopen closed tab"],
        ["Ctrl+W", "Close tab"],
        ["Ctrl+Tab", "Next tab"],
        ["Ctrl+Shift+Tab", "Previous tab"],
        ["Ctrl+L", "Focus address bar"],
        ["Ctrl+R / F5", "Reload"],
        ["Ctrl+P", "Print"],
        ["Ctrl+J", "Open downloads"],
        ["Ctrl+D", "Bookmark this page"],
        ["Middle click", "Close tab"],
        ["Ctrl+\u00b1", "Zoom in / out"],
        ["Ctrl+0", "Reset zoom"],
        ["F11", "Full screen"],
        ["F12", "Developer tools (debug)"],
        ["Alt+Shift+\u2190\u2192\u2191\u2193", "Dock the window"],
        ["Ctrl+Q", "Quit"],
      ];
      for (const [combo, what] of keys) {
        const item = document.createElement("div");
        item.className = "row shortcut-row";
        const key = document.createElement("span");
        key.className = "label";
        key.textContent = combo;
        const text = document.createElement("span");
        text.className = "hint";
        text.textContent = what;
        item.append(key, text);
        appMenu.append(item);
      }
    },
  };

  async function render(page) {
    clear();
    // A page name that does not exist used to throw inside boot, which left an empty
    // panel on screen and one line in the log - the extensions button opened exactly
    // this, because the menu had no `extensions` page to show. Falling back to the root
    // menu at least gives the user something, and the log says which name was wrong.
    const show = pages[page] || pages.root;
    if (!pages[page]) report("menu page missing: " + page + "; showing root");
    await show();
    report("menu page=" + page + " nodes=" + appMenu.children.length);
  }

  // The submenu goes away when the pointer is in neither the row that opened it nor the
  // column it opened: the two are one control, and a pointer crossing the seam between
  // them must not close the thing it is on its way into.
  //
  // `document.body.matches(":hover")` was tried as the test and is always true - the body
  // covers the whole panel - so the submenu never closed at all.
  let leaveTimer = null;

  const scheduleClose = () => {
    clearTimeout(leaveTimer);
    // Long enough for a diagonal move from the row into the column beside it to cross the
    // gap between them, which is what the 8 px between the two boxes is for.
    leaveTimer = setTimeout(() => {
      if (!pointerOverColumn()) closeSub();
    }, 180);
  };

  /// Whether the pointer is on the column or on the row that opened it.
  ///
  /// Both are tracked from the events rather than read off the layout, for the same reason
  /// the row's own state is.
  let overColumn = false;
  const pointerOverColumn = () => overColumn;

  document.addEventListener("pointerover", (event) => {
    const onColumn = Boolean(openSub && openSub.contains(event.target));
    const onHost = Boolean(openHost && openHost.contains(event.target));
    overColumn = onColumn;
    clearTimeout(leaveTimer);
    // Anything that is neither the row nor its column ends the column: this is the whole
    // point of the hover, and it is what keeps the menu from showing a list for a row the
    // pointer has already left.
    if (!onColumn && !onHost) scheduleClose();
  }, true);
  document.addEventListener("pointerout", (event) => {
    if (event.relatedTarget) return;
    overColumn = false;
    scheduleClose();
  }, true);

  appMenu.addEventListener("pointerdown", (event) => {
    if (event.target === appMenu) invoke("overlay_close");
  });

  await render(params.get("page") || "root");

  // The panel opens at the height `initial_size` guesses (566) and is corrected here to the
  // height the rows need, which is the job `fitHeight` does for every other panel - the app
  // menu is excluded from that loop because it reports its own size, so this is its one
  // measurement. Held open for the resize that follows: it takes the focus away from the
  // page, and the blur handler would read that as the pointer leaving and close the menu.
  if (!appMenu.hidden) {
    window.__mixSizing(600);
    requestAnimationFrame(() => {
      const wanted = Math.ceil(menuContentHeight());
      openedHeight = wanted;
      invoke("overlay_resize", { height: wanted }).catch((error) => {
        report(`menu height failed: ${error && error.message ? error.message : error}`);
      });
    });
  }
}
async function bootSuggest() {
  window.addEventListener("blur", () => invoke("overlay_close").catch(() => {}));
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") invoke("overlay_close").catch(() => {});
  });
  document.addEventListener("mousedown", (event) => {
    if (!event.target.closest(".suggest") && !event.target.closest("button")) {
      invoke("overlay_close").catch(() => {});
    }
  });


  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  window.__mixSuggest = (items) => {
    while (menu.firstChild) menu.removeChild(menu.firstChild);
    if (!items.length) {
      const note = document.createElement("div");
      note.className = "menu-note";
      note.textContent = "No matches";
      menu.append(note);
      return;
    }
    for (const item of items) {
      const row = document.createElement("button");
      row.className = "row";
      const icon = document.createElement("span");
      icon.className = "ico";
      icon.textContent = item.kind === "bookmark" ? "\u2606" : "\u21ba";
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = item.title || item.url;
      const host = document.createElement("span");
      host.className = "hint";
      try {
        host.textContent = new URL(item.url).host;
      } catch {
        host.textContent = "";
      }
      row.append(icon, label, host);
      row.addEventListener("click", async () => {
        await invoke("tab_navigate", { url: item.url });
        await invoke("overlay_close");
      });
      menu.append(row);
    }
  };

  window.__mixSuggestActive = (index) => {
    const rows = [...menu.querySelectorAll(".row")];
    rows.forEach((row, at) => row.classList.toggle("active", at === index));
    const active = rows[index];
    if (active && active.scrollIntoView) active.scrollIntoView({ block: "nearest" });
  };

  await invoke("suggest_refresh").catch(() => {});
}

/**
 * Ctrl+Shift+P / Ctrl+Shift+A. Both are the same box: a filterable list that
 * runs something when a row is picked. They live here rather than in the chrome
 * strip because that webview is only as tall as the tab strip.
 */
const PALETTE_ACTIONS = [
  ["New tab", "new_tab"],
  ["New window", "new_window"],
  ["Incognito tab", "new_incognito"],
  ["Reopen closed tab", "reopen_tab"],
  ["Find in page", "find"],
  ["Translate", "translate"],
  ["Zoom in", "zoom_in"],
  ["Zoom out", "zoom_out"],
  ["Reset zoom", "zoom_reset"],
  ["Print", "print"],
  ["Downloads", "open_downloads"],
  ["History", "open_history"],
  ["Bookmarks", "open_bookmarks"],
  ["Extensions", "open_extensions"],
  ["Settings", "open_settings"],
  ["Developer tools", "devtools"],
];

/** Subsequence match with a bonus for runs and word starts. */
function matchScore(text, query) {
  if (!query) return 1;
  const hay = String(text || "").toLowerCase();
  if (hay.startsWith(query)) return 100;
  const direct = hay.indexOf(query);
  if (direct >= 0) return 60 - Math.min(direct, 40) * 0.5;
  let at = 0;
  let points = 0;
  let previous = -2;
  for (const char of query) {
    const found = hay.indexOf(char, at);
    if (found < 0) return 0;
    points += found === previous + 1 ? 3 : 1;
    if (found === 0 || /[\s\/._-]/.test(hay[found - 1] || "")) points += 2;
    previous = found;
    at = found + 1;
  }
  return points;
}

async function bootSearch(mode) {
  const backdrop = document.createElement("div");
  backdrop.id = "palette-backdrop";
  const box = document.createElement("div");
  box.className = "palette";
  const input = document.createElement("input");
  input.className = "palette-input";
  input.placeholder = mode === "tabs" ? t("Search open tabs") : t("Type a command or a site");
  const list = document.createElement("div");
  list.className = "palette-list";
  box.append(input, list);
  backdrop.append(box);
  document.body.append(backdrop);
  input.focus();

  let rows = [];

  const close = () => invoke("overlay_close").catch(() => {});

  const paint = (entries) => {
    rows = entries;
    list.replaceChildren();
    if (!entries.length) {
      const empty = document.createElement("div");
      empty.className = "palette-empty";
      empty.textContent = t("Nothing matches");
      list.append(empty);
      return;
    }
    entries.forEach((entry, index) => {
      const line = document.createElement("div");
      line.className = "palette-row" + (index === 0 ? " active" : "");
      const label = document.createElement("span");
      label.textContent = entry.label;
      line.append(label);
      if (entry.hint) {
        const hint = document.createElement("span");
        hint.className = "palette-hint";
        hint.textContent = entry.hint;
        line.append(hint);
      }
      line.addEventListener("click", () => {
        close();
        entry.run();
      });
      list.append(line);
    });
  };

  const actions = PALETTE_ACTIONS.map(([label, action]) => ({
    label: t(label),
    run: () => invoke("shortcut", { action }).catch(() => {}),
  }));

  const search = async (query) => {
    const text = (query || "").trim().toLowerCase();
    if (mode === "tabs") {
      const tabs = (await invoke("tab_search_all").catch(() => [])) || [];
      const windows = new Set(tabs.map((tab) => tab.window));
      paint(
        tabs
          .map((tab) => ({
            tab,
            points: Math.max(matchScore(tab.title, text), matchScore(tab.url, text) * 0.9),
          }))
          .filter((entry) => entry.points > 0)
          .sort((a, b) => b.points - a.points)
          .slice(0, 40)
          .map(({ tab }) => ({
            label: (tab.pinned ? "\ud83d\udccc " : "") + (tab.title || t("New tab")),
            hint: windows.size > 1 ? tab.url + "  ·  " + tab.window : tab.url,
            run: () => invoke("tab_focus_any", { window: tab.window, id: tab.id }).catch(() => {}),
          })),
      );
      return;
    }

    const found = actions
      .map((action) => ({ action, points: matchScore(action.label, text) }))
      .filter((entry) => entry.points > 0)
      .sort((a, b) => b.points - a.points)
      .slice(0, 8)
      .map(({ action }) => ({ label: action.label, run: action.run }));

    if (text.length >= 2) {
      const history = (await invoke("history_list", { limit: 200 }).catch(() => [])) || [];
      const seen = new Set();
      for (const visit of history.filter((entry) => !seen.has(entry.url) && seen.add(entry.url))) {
        if (found.length >= 14) break;
        if (matchScore(visit.url + " " + (visit.title || ""), text) <= 0) continue;
        found.push({
          label: visit.title || visit.url,
          hint: visit.url,
          run: () => invoke("tab_navigate", { url: visit.url }).catch(() => {}),
        });
      }
    }
    paint(found);
  };

  const move = (step) => {
    const items = [...list.children].filter((row) => row.classList.contains("palette-row"));
    if (!items.length) return;
    const current = items.findIndex((row) => row.classList.contains("active"));
    const next = (current + step + items.length) % items.length;
    items.forEach((row, index) => row.classList.toggle("active", index === next));
    items[next].scrollIntoView({ block: "nearest" });
  };

  const choose = () => {
    const items = [...list.children].filter((row) => row.classList.contains("palette-row"));
    const index = items.findIndex((row) => row.classList.contains("active"));
    const entry = rows[index < 0 ? 0 : index];
    if (!entry) return;
    close();
    entry.run();
  };

  input.addEventListener("input", () => search(input.value));
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      choose();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    }
  });

  await search("");
}

async function bootPalette() {
  await bootSearch("command");
}

async function bootTabSearch() {
  await bootSearch("tabs");
}

/**
 * The tab hover card. The picture can only be taken from a webview that is on
 * screen, so for a background tab it shows the frame from when it was last
 * visited - and everything else on the card is read live: title, address, whether
 * it is loading, playing sound, or suspended.
 */
/** A count, shortened once it gets long enough to be noise. */
function count(value) {
  if (value >= 10000) return Math.round(value / 1000) + "k";
  return String(value);
}

/** Bytes as a short, readable size. */
function megabytes(bytes) {
  const mb = bytes / (1024 * 1024);
  return mb >= 100 ? Math.round(mb) + " MB" : mb.toFixed(1) + " MB";
}



/** The download list, the way a browser shows it from its toolbar. */
async function bootDownloads() {
  const menu = document.getElementById("menu");
  menu.hidden = false;
  const card = document.createElement("div");
  card.className = "download-panel";
  const list = document.createElement("div");
  list.className = "download-list";
  card.append(list);

  const foot = document.createElement("button");
  foot.className = "download-all";
  foot.textContent = t("Show all downloads");
  foot.addEventListener("click", () => {
    invoke("tab_new", { url: "http://tauri.localhost/pages/downloads.html" }).catch(() => {});
    invoke("overlay_close").catch(() => {});
  });
  card.append(foot);
  menu.append(card);

  const size = (bytes) => {
    if (bytes > 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
    if (bytes > 1024) return Math.round(bytes / 1024) + " KB";
    return (bytes || 0) + " B";
  };

  const paint = (tasks) => {
    list.replaceChildren();
    const seen = new Map();
    for (const task of tasks || []) {
      const key = task.path || task.url;
      const rank = (entry) =>
        entry.state === "started" ? 4 : entry.state === "paused" ? 3 : entry.state === "interrupted" ? 2 : 1;
      const held = seen.get(key);
      if (!held || rank(task) >= rank(held)) seen.set(key, task);
    }
    const recent = [...seen.values()].slice(-8).reverse();
    if (!recent.length) {
      const empty = document.createElement("div");
      empty.className = "download-empty";
      empty.textContent = t("No downloads");
      list.append(empty);
      return;
    }
    for (const task of recent) {
      const row = document.createElement("div");
      row.className = "download-row";
      const text = document.createElement("div");
      text.className = "download-text";
      const name = document.createElement("div");
      name.className = "download-name";
      name.textContent = task.file_name || task.url;
      name.title = task.path || task.url;
      const meta = document.createElement("div");
      meta.className = "download-meta";
      const parts = [t(task.state) || task.state];
      if (task.bytes) parts.push(size(task.bytes));
      if (task.state === "started" && task.speed) parts.push(size(task.speed) + "/s");
      if (task.total && task.state !== "finished") {
        parts.push(Math.min(100, Math.round((task.bytes / task.total) * 100)) + "%");
      }
      meta.textContent = parts.join(" \u00b7 ");
      text.append(name, meta);
      if (task.total && task.state !== "finished") {
        const bar = document.createElement("div");
        bar.className = "download-bar";
        const fill = document.createElement("div");
        fill.className = "download-bar-fill";
        fill.style.width = Math.min(100, Math.round((task.bytes / task.total) * 100)) + "%";
        bar.append(fill);
        text.append(bar);
      }
      row.append(text);

      if (task.state === "started" || task.state === "interrupted" || task.state === "paused") {
        const pause = document.createElement("button");
        pause.className = "download-action";
        pause.textContent =
          task.paused || task.state === "interrupted" || task.state === "paused"
            ? t("Resume")
            : t("Pause");
        pause.addEventListener("click", () => {
          invoke("download_pause", { id: task.id, paused: !task.paused }).catch(() => {});
        });
        const cancel = document.createElement("button");
        cancel.className = "download-action";
        cancel.textContent = t("Cancel");
        cancel.addEventListener("click", () => {
          invoke("download_cancel", { id: task.id }).catch(() => {});
        });
        row.append(pause, cancel);
      } else {
        const open = document.createElement("button");
        open.className = "download-action";
        open.textContent = t("Open");
        open.addEventListener("click", () => {
          invoke("download_open", { id: task.id }).catch(() => {});
          invoke("overlay_close").catch(() => {});
        });
        row.append(open);
        if (task.state === "failed" || task.state === "interrupted") {
          const retry = document.createElement("button");
          retry.className = "download-action";
          retry.textContent = t("Download again");
          retry.addEventListener("click", () => {
            invoke("download_retry", { id: task.id }).catch(() => {});
          });
          row.append(retry);
        }
      }
      list.append(row);
    }
  };

  await invoke("download_reconcile").catch(() => {});
  paint(await invoke("download_list").catch(() => []));
  listen("download:changed", (event) => paint(event.payload)).catch(() => {});
}

/** The menu a group chip opens: name, colour, dissolve - as its own overlay. */
async function bootGroupMenu() {
  const id = params.get("id") || "";
  const menu = document.getElementById("menu");
  menu.hidden = false;
  const card = document.createElement("div");
  card.className = "group-menu";
  const field = document.createElement("input");
  field.className = "group-input";
  field.placeholder = t("Group");
  field.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      invoke("group_style", { id, name: field.value, color: null })
        .catch(() => {})
        .finally(() => invoke("overlay_close").catch(() => {}));
    } else if (event.key === "Escape") {
      invoke("overlay_close").catch(() => {});
    }
  });
  card.append(field);

  const palette = document.createElement("div");
  palette.className = "group-palette";
  for (const color of ["#4c8dff", "#39b54a", "#e8a33d", "#e2574c", "#a259d9", "#38b6c4"]) {
    const swatch = document.createElement("button");
    swatch.className = "group-swatch";
    swatch.style.background = color;
    swatch.title = color;
    swatch.addEventListener("click", () => {
      invoke("group_style", { id, name: null, color })
        .catch(() => {})
        .finally(() => invoke("overlay_close").catch(() => {}));
    });
    palette.append(swatch);
  }
  card.append(palette);

  const dissolve = document.createElement("button");
  dissolve.className = "group-menu-item";
  dissolve.textContent = t("Ungroup");
  dissolve.addEventListener("click", () => {
    invoke("group_dissolve", { id })
      .catch(() => {})
      .finally(() => invoke("overlay_close").catch(() => {}));
  });
  card.append(dissolve);
  menu.append(card);
  setLanguage((await invoke("settings_get").catch(() => null))?.language || "en");
  field.focus();
}

/** Puts a picture into the frame and marks the card as having one. */
let thumbFilled = false;
function fillThumb(frame, image, card, src) {
  if (!src || thumbFilled) return;
  thumbFilled = true;
  image.src = src;
  frame.classList.remove("pending");
  card.classList.add("with-image");
}

async function bootThumb() {
  const id = params.get("id") || "";
  thumbFilled = false;
  const ready = listen("thumbs:ready", (event) => {
    const payload = event.payload || {};
    if (payload.id !== id) return;
    const frame = document.querySelector(".thumb-frame");
    const image = document.querySelector(".thumb-image");
    const card = document.querySelector(".thumb-card");
    if (frame && image && card) fillThumb(frame, image, card, payload.image);
  }).catch(() => {});
  void ready;
  const data = await invoke("thumb_card", { id }).catch(() => null);
  if (!data) {
    invoke("overlay_close").catch(() => {});
    return;
  }

  const card = document.createElement("div");
  card.className = "thumb-card" + (data.image ? " with-image" : "");
  card.addEventListener("click", () => {
    invoke("tab_select", { id }).catch(() => {});
    invoke("overlay_close").catch(() => {});
  });

  const frame = document.createElement("div");
  frame.className = "thumb-frame";
  const image = document.createElement("img");
  image.className = "thumb-image";
  if (data.image) {
    image.src = data.image;
  } else {
    frame.classList.add("pending");
    image.alt = "";
  }
  frame.append(image);
  card.append(frame);

  if (data.image) fillThumb(frame, image, card, data.image);

  if (!data.image) {
    (async () => {
      for (let attempt = 0; attempt < 8 && !thumbFilled; attempt += 1) {
        await new Promise((done) => setTimeout(done, 250));
        if (thumbFilled) return;
        const again = await invoke("thumb_card", { id }).catch(() => null);
        if (again && again.image) fillThumb(frame, image, card, again.image);
      }
      if (!thumbFilled) frame.classList.remove("pending");
    })();
  }

  const line = document.createElement("div");
  line.className = "thumb-line";

  const icon = document.createElement("img");
  icon.className = "thumb-icon";
  icon.alt = "";
  if (data.icon) icon.src = data.icon;
  else icon.style.visibility = "hidden";

  const body = document.createElement("div");
  body.className = "thumb-text";
  const title = document.createElement("div");
  title.className = "thumb-title";
  title.textContent = (data.pinned ? "\ud83d\udccc " : "") + (data.title || t("New tab"));
  const url = document.createElement("div");
  url.className = "thumb-url";
  url.textContent = data.url || "";
  body.append(title, url);

  line.append(icon, body);

  const badges = document.createElement("div");
  badges.className = "thumb-badges";
  if (data.loading) {
    const badge = document.createElement("span");
    badge.className = "thumb-badge";
    badge.textContent = t("Loading");
    badges.append(badge);
  }
  if (data.playing || data.muted) {
    const badge = document.createElement("span");
    badge.className = "thumb-badge";
    badge.textContent = data.muted ? "\ud83d\udd07" : "\ud83d\udd0a";
    badges.append(badge);
  }
  if (data.suspended) {
    const badge = document.createElement("span");
    badge.className = "thumb-badge";
    badge.textContent = t("Suspended");
    badges.append(badge);
  }
  if (data.incognito) {
    const badge = document.createElement("span");
    badge.className = "thumb-badge accent";
    badge.textContent = t("Incognito");
    badges.append(badge);
  }
  line.append(badges);
  card.append(line);

  const notes = document.createElement("div");
  notes.className = "thumb-notes";
  card.append(notes);
  invoke("thumb_stats", { id })
    .then((stats) => {
      if (!stats) return;
      const pageParts = [];
      if (stats.memory_tab) pageParts.push(megabytes(stats.memory_tab));
      if (stats.nodes) pageParts.push(count(stats.nodes) + " " + t("nodes"));
      if (stats.listeners) pageParts.push(count(stats.listeners) + " " + t("listeners"));
      if (pageParts.length) {
        const line = document.createElement("div");
        line.className = "thumb-note thumb-memory";
        line.textContent = t("Page") + " " + pageParts.join(" · ");
        notes.append(line);
      }
      const totalParts = [];
      if (stats.memory_total) {
        totalParts.push(t("Browser") + " " + megabytes(stats.memory_total));
      }
      if (stats.memory_average && stats.live_tabs > 1) {
        totalParts.push(
          t("Average") + " " + megabytes(stats.memory_average) + " / " + stats.live_tabs,
        );
      }
      if (totalParts.length) {
        const line = document.createElement("div");
        line.className = "thumb-note thumb-memory";
        line.textContent = totalParts.join("  ·  ");
        notes.append(line);
      }
    })
    .catch(() => {});

  if (data.image && data.image_age_secs !== null && data.image_age_secs > 8) {
    const note = document.createElement("div");
    note.className = "thumb-note";
    note.textContent = t("Picture from when this tab was last open");
    card.append(note);
  }

  document.body.append(card);
}

async function bootZoom() {
  // A single row of controls: the panel only needs the room that row takes.
  document.body.classList.add("zoom-panel");
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const bar = document.createElement("div");
  bar.className = "zoom-bar";

  const minus = document.createElement("button");
  minus.className = "find-step";
  minus.textContent = "\u2212";
  const level = document.createElement("button");
  level.className = "zoom-level";
  level.title = "Reset to 100%";
  const plus = document.createElement("button");
  plus.className = "find-step";
  plus.textContent = "+";

  let zoom = await invoke("zoom_get");
  const paint = (value) => {
    zoom = value;
    level.textContent = Math.round(value * 100) + "%";
  };
  paint(zoom);
  // The wheel and the keyboard change the zoom too, so the panel takes the new value.
  window.__mixZoom = (value) => paint(value);

  minus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: -0.1 })));
  plus.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 0.1 })));
  level.addEventListener("click", async () => paint(await invoke("zoom_step", { delta: 1 - zoom })));

  bar.append(minus, level, plus);
  menu.append(bar);
}

/// The address of the page as a code a phone can read off the screen.
async function bootQr() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const bar = document.createElement("div");
  bar.className = "qr";
  // The command hands back svg markup, which goes on the page as markup: a data url is
  // refused by the webview, and this is the same picture without one.
  try {
    const markup = await invoke("page_qr");
    bar.innerHTML = markup;
    const image = bar.querySelector("svg");
    if (image) {
      image.setAttribute("width", "100%");
      image.setAttribute("height", "100%");
    }
  } catch (error) {
    report("qr failed: " + String(error));
  }
  menu.append(bar);

  menu.addEventListener("pointerdown", (event) => {
    if (event.target === menu) invoke("overlay_close");
  });
}

async function bootFind() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const bar = document.createElement("div");
  bar.className = "find-bar";
  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = t("Find in page");
  input.className = "find-input";
  const prev = document.createElement("button");
  prev.className = "find-step";
  prev.textContent = "\u2191";
  const next = document.createElement("button");
  next.className = "find-step";
  next.textContent = "\u2193";
  bar.append(input, prev, next);
  menu.append(bar);

  const status = document.createElement("span");
  status.className = "find-status";
  bar.append(status);

  const search = async (forward, restart) => {
    if (!input.value) return;
    await invoke("find_in_page", { text: input.value, forward, restart }).catch(() => {});
    await new Promise((resolve) => setTimeout(resolve, 120));
    const hit = await invoke("find_status").catch(() => true);
    status.textContent = hit ? "" : t("No matches");
  };

  input.addEventListener("input", () => search(true, true));
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      search(!event.shiftKey, false);
    }
  });
  next.addEventListener("click", () => search(true, false));
  prev.addEventListener("click", () => search(false, false));

  setTimeout(() => input.focus(), 30);
}

async function bootExtInfo() {
  appMenu.hidden = false;
  themePanel.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const wanted = params.get("name") || "";
  const list = await invoke("ext_list");
  const ext = list.find((item) => item.name === wanted) || list[0];
  if (!ext) return;

  const head = document.createElement("div");
  head.className = "menu-header";
  if (ext.icon) {
    const img = document.createElement("img");
    img.src = ext.icon;
    img.alt = "";
    img.className = "ext-badge";
    head.append(img);
  }
  const title = document.createElement("span");
  title.textContent = ext.version ? `${ext.name} ${ext.version}` : ext.name;
  head.append(title);
  appMenu.append(head);

  const note = (text) => {
    if (!text) return;
    const line = document.createElement("div");
    line.className = "menu-note";
    line.textContent = text;
    appMenu.append(line);
  };
  note(ext.description || "");
  note(ext.popup ? `Popup: ${ext.popup}` : "No toolbar popup");
  if (ext.permissions && ext.permissions.length) {
    note(`${t("Permissions")}: ${ext.permissions.slice(0, 8).join(", ")}`);
  }

  const divider = () =>
    appMenu.append(Object.assign(document.createElement("div"), { className: "divider" }));
  divider();

  const action = (glyph, label, run) => {
    const button = document.createElement("button");
    button.className = "row";
    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = t(label);
    button.append(icon, text, document.createElement("span"));
    button.addEventListener("click", run);
    appMenu.append(button);
  };

  action("\u25a4", "Open extension folder", async () => {
    await invoke("open_dir", { which: "extensions" });
    await close();
  });
  action("\u21ba", "Reload extensions", async () => {
    report("Extensions reload on restart");
    await close();
  });
}

async function bootChromeMenu(kind) {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const item = (label, run) => {
    const button = document.createElement("button");
    button.className = "row plain";
    button.textContent = t(label);
    button.addEventListener("click", async () => {
      await run();
      await invoke("overlay_close").catch(() => {});
    });
    menu.append(button);
  };

  if (kind === "url") {
    item("Cut", () => invoke("url_action", { action: "cut" }));
    item("Copy", () => invoke("url_action", { action: "copy" }));
    item("Paste", () => invoke("url_action", { action: "paste" }));
    const divider = document.createElement("div");
    divider.className = "divider";
    menu.append(divider);
    item("Select all", () => invoke("url_action", { action: "select-all" }));
    return;
  }

  // The contents of one folder of the bookmarks bar.
  //
  // The bar draws a folder as one button, and this is what that button opens: the pages the
  // folder holds, in the order the bar would have drawn them. Entries are ordinary rows so a
  // list of twenty is readable, and each one opens through the same setting the bar uses.
  if (kind === "bookmark") {
    const folder = params.get("folder") || "";
    const marks = (await invoke("bookmark_list").catch(() => [])) || [];
    const inside = marks.filter((mark) => (mark.folder || "") === folder);
    if (!inside.length) {
      const note = document.createElement("div");
      note.className = "menu-note";
      note.textContent = t("This folder is empty.");
      menu.append(note);
      return;
    }
    // A heading with the folder's name, and its count, so a column of rows says which list
    // it is without the reader having to remember which button they pressed.
    const head = document.createElement("div");
    head.className = "menu-header";
    const label = document.createElement("span");
    label.textContent = folder || t("Bookmarks");
    const tally = document.createElement("span");
    tally.className = "hint";
    tally.textContent = String(inside.length);
    head.append(label, tally);
    menu.append(head);

    for (const mark of inside) {
      const row = document.createElement("button");
      row.className = "row";
      row.title = mark.url;
      const icon = document.createElement("span");
      icon.className = "ico";
      icon.textContent = (hostOf(mark.url)[0] || "?").toUpperCase();
      const text = document.createElement("span");
      text.className = "label";
      text.textContent = mark.title || mark.url;
      row.append(icon, text);
      row.addEventListener("click", async () => {
        const where = params.get("where") || "foreground";
        if (where === "current") {
          await invoke("tab_navigate", { url: mark.url }).catch(() => {});
        } else {
          await invoke("tab_new", { url: mark.url, background: where === "background" }).catch(
            () => {},
          );
        }
        await invoke("overlay_close").catch(() => {});
      });
      menu.append(row);
    }
    return;
  }

  if (kind === "tab") {
    const context = await invoke("menu_context_get").catch(() => null);
    const id = context && context.tab;
    const list = (await invoke("tab_list").catch(() => null)) || { tabs: [] };
    const tab = (list.tabs || []).find((entry) => entry.id === id);
    if (!id || !tab) return;
    item(tab.pinned ? "Unpin tab" : "Pin tab", () =>
      invoke("tab_pin", { id, pinned: !tab.pinned }),
    );
    item("Reload", () => invoke("tab_nav", { action: "reload" }));
    item("Group or ungroup", async () => {
      const group = (list.groups || []).find((entry) => entry.members.includes(id));
      if (group) await invoke("group_remove", { id }).catch(() => {});
      else await invoke("group_add", { id, groupId: null }).catch(() => {});
    });
    item("Copy address", () => invoke("clipboard_write", { text: tab.url }).catch(() => {}));
    item("Capture full page", () => invoke("tab_screenshot", { full: true }).catch(() => {}));
    const divider = document.createElement("div");
    divider.className = "divider";
    menu.append(divider);
    // `from_menu` is what lets a pinned tab close at all: the command refuses a pinned tab
    // from every other route when the setting asks for that.
    item("Close tab", () => invoke("tab_close", { id, from_menu: true }).catch(() => {}));
    return;
  }

  item("New tab", () => invoke("tab_new", { url: null }));
  item("Reload", () => invoke("tab_nav", { action: "reload" }));
  const divider = document.createElement("div");
  divider.className = "divider";
  menu.append(divider);
  item("Settings", () => invoke("overlay_open", { kind: "app_menu&page=settings" }));
}

async function bootMenu() {
  menu.hidden = false;
  appMenu.hidden = true;
  themePanel.hidden = true;
  applyTheme(await invoke("theme_get"));

  const context = await invoke("menu_context_get");

  const divider = () => menu.append(Object.assign(document.createElement("div"), { className: "divider" }));

  const item = (glyph, label, hint, run) => {
    const button = document.createElement("button");
    button.className = "row";
    const icon = document.createElement("span");
    icon.className = "ico";
    icon.textContent = glyph;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = t(label);
    const key = document.createElement("span");
    key.className = "hint";
    key.textContent = hint || "";
    button.append(icon, text, key);
    button.addEventListener("click", run);
    menu.append(button);
    return button;
  };

  const copy = async (value) => {
    try {
      await navigator.clipboard.writeText(value || "");
    } catch (error) {
      report("clipboard failed: " + error);
    }
  };

  if (context.link) {
    item("\u2197", "Open link in new tab", "", () => invoke("tab_new", { url: context.link }));
    item("\u2398", "Copy link address", "", () => copy(context.link));
    divider();
  }
  if (context.image) {
    item("\u25a3", "Open image in new tab", "", () => invoke("tab_new", { url: context.image }));
    item("\u2398", "Copy image address", "", () => copy(context.image));
    // Reverse image search is a third-party request with the picture's address in it, so
    // the row says whose engine takes it rather than leaving that to be guessed.
    item("\u2315", "Search this image on Google", "", async () => {
      const url = "https://www.google.com/searchbyimage?image_url=" + encodeURIComponent(context.image);
      await invoke("tab_new", { url });
      await invoke("overlay_close");
    });
    item("\u2b07", "Save image as...", "", async () => {
      const name = (context.image.split("/").pop() || "image").split("?")[0];
      try {
        await invoke("download_url", { url: context.image, fileName: name });
      } catch (error) {
        report("save image failed: " + error);
      }
      await invoke("overlay_close");
    });
    divider();
  }
  if (context.selection) {
    item("\u2398", "Copy", "", () => copy(context.selection));
    divider();
  }
  if (context.editable) {
    item("\u2702", "Cut", "", () => document.execCommand("cut"));
    item("\u2398", "Copy", "", () => document.execCommand("copy"));
    item("\u2399", "Paste", "", () => document.execCommand("paste"));
    divider();
  }

  // The keys are shown because they work from here: the strip's own handler does not
  // depend on the menu, and a menu that hides what the keyboard can do reads as a list of
  // the only things a mouse may press.
  item("\u2190", "Back", "Alt+\u2190", () => invoke("tab_nav", { action: "back" }));
  item("\u2192", "Forward", "Alt+\u2192", () => invoke("tab_nav", { action: "forward" }));
  item("\u21bb", "Reload", "Ctrl+R", () => invoke("tab_nav", { action: "reload" }));
  item("\u2606", "Bookmark this page", "Ctrl+D", () => invoke("shortcut", { action: "bookmark" }));
  divider();
  item("\u2399", "Print", "Ctrl+P", () => invoke("tab_print"));
  item("\u2b07", "Save page as...", "Ctrl+S", async () => {
    await invoke("save_page").catch((error) => report(String(error)));
    await invoke("overlay_close");
  });
  if (context.selection) {
    item("\u4e16", "Translate selection", "", async () => {
      await invoke("translate_page", { target: context.selection }).catch((error) => report(String(error)));
      await invoke("overlay_close");
    });
  }
  item("\u4e16", "Translate page", "", async () => {
    await invoke("translate_page", { target: null }).catch((error) => report(String(error)));
    await invoke("overlay_close");
  });
  item("\u25a6", "QR code for this page", "", async () => {
    await invoke("overlay_open", { kind: "qr" }).catch((error) => report(String(error)));
  });
  item("\u2261", "Open in reading mode", "", async () => {
    await invoke("reading_mode").catch((error) => report(String(error)));
    await invoke("overlay_close");
  });
  item("\u2318", "View source", "Ctrl+U", async () => {
    await invoke("view_source");
    await invoke("overlay_close");
  });
  item("\u2691", "Inspect", "", () => invoke("shortcut", { action: "devtools" }));

  menu.addEventListener("pointerdown", (event) => {
    if (event.target === menu) invoke("overlay_close");
  });
}

async function bootPermission() {
  permissionPanel.hidden = false;
  themePanel.hidden = true;
  appMenu.hidden = true;
  menu.hidden = true;
  applyTheme(await invoke("theme_get"));

  const id = Number(params.get("id") || 0);
  const origin = params.get("origin") || "this page";
  const name = params.get("kind") || "something";

  const text = document.getElementById("permission-text");
  text.textContent = `${origin} wants to use ${name}.`;

  const answer = async (allow) => {
    await invoke("permission_reply", { id, allow });
  };
  document.getElementById("permission-allow").addEventListener("click", () => answer(true));
  document.getElementById("permission-block").addEventListener("click", () => answer(false));
}

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    invoke("overlay_close").catch(() => {});
  }
});

async function applyLanguage() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  // The theme panel and the permission prompt are markup, not rows built in js.
  applyStatic();
}

async function bootPanel() {
  menu.hidden = true;
  appMenu.hidden = true;
  themePanel.hidden = true;
  permissionPanel.hidden = true;
  await applyLanguage();
  report(`overlay kind=${kind}`);
  if (kind === "app_menu") {
    await bootAppMenu();
    return;
  }
  if (kind === "context_menu") {
    await bootMenu();
    return;
  }
  if (kind === "thumb") {
    await bootThumb();
    return;
  }
  if (kind === "group_menu") {
    await bootGroupMenu();
    return;
  }
  if (kind === "downloads") {
    await bootDownloads();
    return;
  }
  if (kind === "palette") {
    await bootPalette();
    return;
  }
  if (kind === "tab_search") {
    await bootTabSearch();
    return;
  }
  if (kind === "zoom") {
    await bootZoom();
    return;
  }
  if (kind === "qr") {
    await bootQr();
    return;
  }
  if (kind === "find") {
    await bootFind();
    return;
  }
  if (kind === "url_menu") {
    await bootChromeMenu("url");
    return;
  }
  if (kind === "chrome_menu") {
    await bootChromeMenu("chrome");
    return;
  }
  if (kind === "tab_menu") {
    await bootChromeMenu("tab");
    return;
  }
  if (kind === "bookmark_menu") {
    await bootChromeMenu("bookmark");
    return;
  }
  if (kind === "ext_info") {
    await bootExtInfo();
    return;
  }
  if (kind === "suggest") {
    await bootSuggest();
    return;
  }
  if (kind === "permission") {
    await bootPermission();
    return;
  }

  const section = params.get("section") || "appearance";
  const titles = { appearance: "Appearance", startup: "Startup", theme: "Theme file" };
  document.getElementById("panel-title").textContent = t(titles[section] || "Settings");
  for (const block of document.querySelectorAll("[data-section]")) {
    block.hidden = block.dataset.section !== section;
  }
  document.getElementById("presets").hidden = section !== "appearance";
  document.getElementById("panel-back").addEventListener("click", () =>
    invoke("overlay_open", { kind: "app_menu&page=settings" }),
  );

  current = await invoke("theme_get");
  render(current);

  autostartBox.checked = await invoke("autostart_get");

  const settings = await invoke("settings_get");
  restoreBox.checked = !!settings.restore_session;

  for (const input of Object.values(fields)) input.addEventListener("input", preview);
  radius.addEventListener("input", preview);
  tabRadius.addEventListener("input", preview);
  fontSize.addEventListener("input", preview);
  fontFamily.addEventListener("change", preview);

  for (const button of document.querySelectorAll("[data-preset]")) {
    button.addEventListener("click", async () => {
      const preset = PRESETS[button.dataset.preset];
      current = await invoke("theme_apply", {
        theme: { ...current, colors: { ...current.colors, ...preset }, radius: preset.radius },
        persist: false,
      });
      render(current);
    });
  }

  const themeError = document.getElementById("theme-error");

  document.getElementById("theme-export").addEventListener("click", async () => {
    try {
      const path = await invoke("theme_export");
      themeError.textContent = "";
      report("theme exported to " + path);
    } catch (error) {
      themeError.textContent = String(error);
    }
  });

  document.getElementById("theme-apply-json").addEventListener("click", async () => {
    const box = document.getElementById("theme-json");
    try {
      current = await invoke("theme_import", { json: box.value });
      render(current);
      box.value = "";
      themeError.textContent = "";
    } catch (error) {
      themeError.textContent = String(error);
    }
  });

  document.getElementById("save").addEventListener("click", async () => {
    await invoke("theme_apply", { theme: current, persist: true });
    await invoke("settings_set", { patch: { restore_session: restoreBox.checked } });
    await invoke("autostart_set", { enabled: autostartBox.checked });
    await close();
  });

  document.getElementById("cancel").addEventListener("click", async () => {
    await invoke("theme_revert");
    await close();
  });

  document.addEventListener("keydown", async (event) => {
    if (event.key === "Escape") {
      await invoke("theme_revert");
      await close();
    }
  });
}

/** Panels whose size is the design rather than a measurement: Rust keeps it. */
const FIXED_SIZE = new Set(["thumb", "palette", "tab_search", "settings"]);

/// Panels that size themselves instead of being measured by `fitHeight`.
///
/// The app menu reports its own height, because it is the one panel whose height and width
/// are decided together: it opens at the height its rows need and the panel it is drawn in
/// is wide enough for the column beside it. The generic measurement below would fight it -
/// it reports a height with no width, and the command keeps the current width for that, so
/// the two loops used to rewrite each other's answer several times a second. The visible
/// symptom was the column's scrollbar changing width and colour between the two sizes, and,
/// once the width lost, the menu being pushed out of a viewport narrower than the
/// `left: 308px` it is drawn at - a panel showing only the history column.
const SELF_SIZED = new Set(["app_menu"]);

/** The panel the boot function just revealed, if it revealed one. */
function visiblePanel() {
  return [menu, appMenu, themePanel, permissionPanel].find(
    (panel) => !panel.hidden && panel.childElementCount > 0,
  );
}

/**
 * Reports the height the panel's content needs, and reports it again whenever that
 * changes: the window is resized to match, so the page has to measure its own content
 * instead of describing itself as "as tall as the window".
 */
function fitHeight(panel) {
  let sent = 0;
  let queued = 0;
  let failed = false;
  const measure = () => {
    queued = 0;
    panel.style.height = "auto";
    panel.style.maxHeight = "none";
    const height = Math.ceil(panel.getBoundingClientRect().height);
    if (height < 1 || Math.abs(height - sent) < 1) return;
    sent = height;
    // The measured height stays as the cap, so a panel with more rows than the window
    // can show scrolls inside it instead of losing its first rows off the top.
    panel.style.maxHeight = `${height}px`;
    invoke("overlay_resize", { height }).catch((error) => {
      if (failed) return;
      failed = true;
      report(`overlay resize failed: ${error && error.message ? error.message : error}`);
    });
  };
  const soon = () => {
    if (!queued) queued = requestAnimationFrame(measure);
  };
  measure();
  new ResizeObserver(soon).observe(panel);
}

async function boot() {
  await bootPanel();
  if (FIXED_SIZE.has(kind) || SELF_SIZED.has(kind)) return;
  const panel = visiblePanel();
  if (panel) fitHeight(panel);
}

boot().catch((error) => {
  console.error("overlay boot failed:", error);
  report(`overlay boot failed: ${error && error.message ? error.message : error}`);
});