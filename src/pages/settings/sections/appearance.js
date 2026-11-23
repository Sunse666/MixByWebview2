// Appearance: the palette, the toolbar, the tab strip, the sidebar and the page's type.
//
// One card, in the order a reader goes looking: what the window looks like, which buttons
// the strip carries, then how tabs and the sidebar behave, then the page itself. The order
// and the wording follow the browser this one imitates, because a person who has used that
// one should not have to learn a second vocabulary for the same five decisions.
import { t } from "../../../i18n.js";
import { row, switchEl, themePreview } from "../ui.js";
import { SIZE_PRESETS, ZOOM_LEVELS, TOOLBAR_CHOICES, DEFAULT_TOOLBAR } from "../tables.js";

export function buildAppearance({ settings, invoke }) {
  const save = (patch) => invoke("settings_set", { patch }).catch(() => {});

  /// A select over [value, label] pairs, handing the value to the caller.
  ///
  /// Three of these rows store a number while the option carries a string, and one edits a
  /// list rather than a key, so the write belongs to the row instead of being implied by a
  /// key name here.
  const choice = (options, selected, onChange) => {
    const select = document.createElement("select");
    select.className = "button";
    for (const [value, label] of options) {
      const option = document.createElement("option");
      option.value = String(value);
      option.textContent = t(label);
      if (String(selected) === String(value)) option.selected = true;
      select.append(option);
    }
    select.addEventListener("change", () => onChange(select.value));
    return select;
  };

  /// A switch bound to one boolean setting.
  const switchRow = (label, key) =>
    row(
      label,
      "",
      switchEl(Boolean(settings[key]), (on) => save({ [key]: on })),
    );

  /// An icon button, the kind a row uses to open something rather than to set a value.
  const iconButton = (glyph, title, onClick) => {
    const button = document.createElement("button");
    button.className = "button icon-button";
    button.textContent = glyph;
    button.title = t(title);
    button.addEventListener("click", onClick);
    return button;
  };

  // --- toolbar ------------------------------------------------------------
  // The list is the setting: a button that is on is in it, and the strip reads it in order.
  const toolbarIds = () =>
    Array.isArray(settings.toolbar) && settings.toolbar.length
      ? [...settings.toolbar]
      : [...DEFAULT_TOOLBAR];
  const saveToolbar = async (next) => {
    settings.toolbar = next;
    await save({ toolbar: next });
  };

  const resetToolbar = document.createElement("button");
  resetToolbar.className = "button";
  resetToolbar.textContent = t("Reset to defaults");
  resetToolbar.addEventListener("click", () => saveToolbar([...DEFAULT_TOOLBAR]));

  // Where the bookmark button sits: the two ends of the strip. It is one entry in the list
  // above, so this only moves it to the front or to the back.
  const bookmarkPlace = choice(
    [
      ["left", "Show on the left"],
      ["right", "Show on the right"],
    ],
    toolbarIds()[0] === "bookmark" ? "left" : "right",
    async (value) => {
      const list = toolbarIds().filter((item) => item !== "bookmark");
      if (bookmarkBox.checked) {
        if (value === "left") list.unshift("bookmark");
        else list.push("bookmark");
      }
      await saveToolbar(list);
    },
  );

  // The bookmark button's own row: a tick means the button is in the strip at all, and the
  // select beside it says which end. A switch would read as "on/off" for the whole row,
  // which is why this one is the checkbox the browser uses - the select is the value.
  const bookmarkBox = document.createElement("input");
  bookmarkBox.type = "checkbox";
  bookmarkBox.checked = toolbarIds().includes("bookmark");
  bookmarkBox.addEventListener("change", async () => {
    const list = toolbarIds().filter((item) => item !== "bookmark");
    if (bookmarkBox.checked) {
      if (bookmarkPlace.value === "left") list.unshift("bookmark");
      else list.push("bookmark");
    }
    bookmarkPlace.disabled = !bookmarkBox.checked;
    await saveToolbar(list);
  });
  bookmarkPlace.disabled = !bookmarkBox.checked;
  const bookmarkRow = document.createElement("div");
  bookmarkRow.className = "row-inline";
  bookmarkRow.append(bookmarkBox, bookmarkPlace);

  // --- tabs ---------------------------------------------------------------
  const searchPlace = choice(
    [
      ["right", "Show on the right"],
      ["left", "Show on the left"],
      ["center", "Show in the middle"],
    ],
    settings.tab_search_place || "right",
    (value) => save({ tab_search_place: value }),
  );

  // --- sidebar ------------------------------------------------------------
  const sideSelect = choice(
    [
      ["off", "Off"],
      ["left", "Show on the left"],
      ["right", "Show on the right"],
    ],
    settings.sidebar || "off",
    (value) => save({ sidebar: value }),
  );

  // --- the page -----------------------------------------------------------
  const zoomSelect = choice(
    ZOOM_LEVELS.map((level) => [String(level), Math.round(level * 100) + "%"]),
    String(settings.default_zoom ?? 1),
    (value) => save({ default_zoom: Number(value) }),
  );

  const sizeSelect = choice(
    SIZE_PRESETS.map(([size, label]) => [
      String(size),
      size === 0 ? t(label) : `${t(label)} (${size}px)`,
    ]),
    String(settings.font_size ?? 0),
    (value) => save({ font_size: Number(value) }),
  );

  // The fonts live behind one row: a name typed into a field is a decision made once and
  // then forgotten, so the two inputs open under the row rather than sitting on the card.
  //
  // Each field is a text input over a `<datalist>`, not a `<select>`: the list of installed
  // families is long, a reader knows the name of the one they want, and a value that is not
  // installed still has to be accepted - a page can name a font the system does not have and
  // let the site's own stack decide. A select would rule both of those out.
  const fontPanel = document.createElement("div");
  fontPanel.className = "card-actions";
  fontPanel.hidden = true;

  /// A font field: free text, with the installed families offered as it is typed.
  ///
  /// `datalist` matches on a prefix of the value, so "Source" finds "Source Code Pro" while
  /// the field stays editable. The list is filled once, when the panel first opens.
  const fontField = (key, value, placeholder, listId) => {
    const input = document.createElement("input");
    input.className = "hex";
    input.style.width = "240px";
    input.placeholder = placeholder;
    input.value = value || "";
    input.setAttribute("list", listId);
    input.addEventListener("change", () => save({ [key]: input.value.trim() }));

    const list = document.createElement("datalist");
    list.id = listId;
    fontPanel.append(list);
    return { input, list };
  };

  const fontFieldOf = fontField("font_family", settings.font_family, "Microsoft YaHei", "font-list-family");
  const monoFieldOf = fontField("font_mono", settings.font_mono, "JetBrains Mono", "font-list-mono");

  fontPanel.append(
    row(t("Standard font"), t("Empty keeps the site font"), fontFieldOf.input),
    row(t("Monospace font"), t("Used for code blocks and text areas"), monoFieldOf.input),
  );

  // Filled on first open rather than at build: the list is a few hundred entries and most
  // visits to this page never touch the font rows.
  let fontsLoaded = false;
  const fillFonts = async () => {
    if (fontsLoaded) return;
    fontsLoaded = true;
    const families = (await invoke("font_list").catch(() => [])) || [];
    for (const { list } of [fontFieldOf, monoFieldOf]) {
      list.replaceChildren(
        ...families.map((family) => {
          const option = document.createElement("option");
          // The value is the family name, which is what goes into `font-family`. The label
          // is the same name plus its own language's name, when it has one, so a reader who
          // knows "微软雅黑" can find "Microsoft YaHei" by that name too.
          option.value = family.name;
          option.label = family.local && family.local !== family.name
            ? `${family.name} · ${family.local}`
            : family.name;
          return option;
        }),
      );
    }
  };

  const fontToggle = iconButton("\u203a", "Custom fonts", () => {
    fontPanel.hidden = !fontPanel.hidden;
    if (!fontPanel.hidden) fillFonts();
  });

  // --- the section --------------------------------------------------------
  const head = document.createElement("h2");
  head.textContent = t("Appearance");
  head.style.margin = "18px 0 10px";

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  const modeSelect = choice(
    [
      ["dark", "Dark"],
      ["light", "Light"],
      ["system", "Follow system"],
    ],
    settings.theme_mode || "dark",
    (value) => {
      save({ theme_mode: value });
      location.reload();
    },
  );

  // `themePreview` opens the colour and radius editor in place of the buttons it is given,
  // and puts them back when the editor is finished or cancelled.
  const themeActions = document.createElement("div");
  themeActions.className = "card-actions";
  themeActions.append(themePreview());

  const toolbarActions = document.createElement("div");
  toolbarActions.className = "row-inline";
  toolbarActions.append(iconButton("\u270e", "Customize toolbar", () => {
    // The switches below the row are the editor; this only scrolls to them, which is what
    // the icon means in the browser this one imitates.
    card.querySelector('[data-toolbar-start="1"]')?.scrollIntoView({ block: "nearest" });
  }), resetToolbar);

  const toolbarRows = TOOLBAR_CHOICES.map(([id, label]) =>
    row(
      label,
      "",
      switchEl(toolbarIds().includes(id), async (on) => {
        const list = toolbarIds().filter((item) => item !== id);
        if (on) list.push(id);
        await saveToolbar(list);
      }),
    ),
  );
  if (toolbarRows.length) toolbarRows[0].dataset.toolbarStart = "1";

  card.append(
    row(t("Theme"), t("Colours, corner radius and the interface font"), themeActions),
    row(t("Customize toolbar"), "", toolbarActions),
    row(t("Mode"), "", modeSelect),
    ...toolbarRows,
    row(t("Bookmark button"), "", bookmarkRow),
    switchRow("Show tab groups in the bookmarks bar", "groups_in_bookmarks"),
    switchRow("Pin a new group to the bookmarks bar", "group_auto_pin"),
    row(t("Tab search position"), "", searchPlace),
    row(t("Sidebar position"), "", sideSelect),
    row(t("Tab hover preview"), t("Show a card when the pointer rests on a tab"), switchEl(Boolean(settings.tab_hover_preview), (on) => save({ tab_hover_preview: on }))),
    row(t("Show the tab preview image"), "", switchEl(Boolean(settings.live_thumbs), (on) => save({ live_thumbs: on }))),
    row(t("Show the tab's memory use"), "", switchEl(settings.thumb_show_memory !== false, (on) => save({ thumb_show_memory: on }))),
    row(t("Font size"), "", sizeSelect),
    row(t("Custom fonts"), "", fontToggle),
    fontPanel,
    row(t("Page zoom"), "", zoomSelect),
  );

  return [head, card];
}
