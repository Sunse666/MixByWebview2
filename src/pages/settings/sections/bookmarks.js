// Bookmarks: how the bar appears and what its buttons do.
//
// The order follows the screenshot this page restores: what the menu of folders looks like,
// then when the bar shows itself, then what a button draws and what a click on it does.
import { t } from "../../../i18n.js";
import { row, switchEl, slider, selectField } from "../ui.js";

export function buildBookmarks({ settings, invoke }) {
  const save = (patch) => invoke("settings_set", { patch }).catch(() => {});

  /// A select over [value, label] pairs whose write is the caller's.
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

  const head = document.createElement("h2");
  head.textContent = t("Bookmarks");
  head.style.margin = "18px 0 10px";

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  // --- the menu of folders ------------------------------------------------
  // "Several columns, each this wide": the folders of the bar open into a list, and this is
  // the width one of those columns gets. It is a switch as well as a width because a reader
  // who wants one plain menu should not have to set a number to say so.
  const columns = slider(
    Math.round(settings.bookmark_column_width ?? 200),
    120,
    400,
    (value) => save({ bookmark_column_width: value }),
  );
  const columnsRow = document.createElement("div");
  columnsRow.className = "row-inline";
  const columnsBox = document.createElement("input");
  columnsBox.type = "checkbox";
  columnsBox.checked = settings.bookmark_menu_columns !== false;
  columnsBox.addEventListener("change", async () => {
    columns.disabled = !columnsBox.checked;
    await save({ bookmark_menu_columns: columnsBox.checked });
  });
  columns.disabled = !columnsBox.checked;
  columnsRow.append(columnsBox, columns.input, columns.value);

  // --- revealing the bar at the edge --------------------------------------
  // Three numbers that only mean anything together: how close to the left edge the pointer
  // has to get, how long it has to rest there, and how long after leaving the bar goes
  // away. Laid out as a sentence because that is how the browser this copies words it, and
  // because three labelled fields for one behaviour read as three behaviours.
  const edgeField = document.createElement("input");
  edgeField.className = "hex num";
  edgeField.type = "number";
  edgeField.min = "0";
  edgeField.max = "200";
  edgeField.value = String(settings.bookmark_reveal_edge ?? 0);
  edgeField.addEventListener("change", () =>
    save({ bookmark_reveal_edge: Number(edgeField.value) || 0 }),
  );

  const delayField = document.createElement("input");
  delayField.className = "hex num";
  delayField.type = "number";
  delayField.min = "50";
  delayField.max = "2000";
  delayField.value = String(settings.bookmark_reveal_delay ?? 250);
  delayField.addEventListener("change", () =>
    save({ bookmark_reveal_delay: Number(delayField.value) || 250 }),
  );

  const hideField = document.createElement("input");
  hideField.className = "hex num";
  hideField.type = "number";
  hideField.min = "100";
  hideField.max = "5000";
  hideField.value = String(settings.bookmark_hide_delay ?? 1200);
  hideField.addEventListener("change", () =>
    save({ bookmark_hide_delay: Number(hideField.value) || 1200 }),
  );

  const revealRow = document.createElement("div");
  revealRow.className = "sentence";
  const revealBox = document.createElement("input");
  revealBox.type = "checkbox";
  revealBox.checked = (settings.bookmark_reveal_edge ?? 0) > 0;
  const syncReveal = () => {
    edgeField.disabled = !revealBox.checked;
    delayField.disabled = !revealBox.checked;
    hideField.disabled = !revealBox.checked;
    if (!revealBox.checked) return;
  };
  revealBox.addEventListener("change", async () => {
    // A checkbox cannot carry a distance, so ticking it fills in the number the sentence
    // already shows; unticking it writes zero, which is what "off" is.
    if (revealBox.checked && !Number(edgeField.value)) edgeField.value = "10";
    if (!revealBox.checked) edgeField.value = "0";
    syncReveal();
    await save({ bookmark_reveal_edge: Number(edgeField.value) || 0 });
  });
  syncReveal();

  /// One run of words inside the sentence row.
  const words = (text) => {
    const span = document.createElement("span");
    span.textContent = t(text);
    return span;
  };

  revealRow.append(
    revealBox,
    words("Reveal the bar when the pointer rests"),
    edgeField,
    words("pixels inside the left edge for"),
    delayField,
    words("ms, and hide it"),
    hideField,
    words("ms after it leaves"),
  );

  // --- what a button looks like -------------------------------------------
  const width = slider(
    Math.round(settings.bookmark_button_width ?? 132),
    40,
    320,
    (value) => save({ bookmark_button_width: value }),
  );

  // --- how the bar appears ------------------------------------------------
  const barMode = selectField(
    invoke,
    [
      ["never", "Always hidden"],
      ["address", "While the address bar has the keyboard"],
      ["always", "Always show"],
    ],
    settings.bookmark_bar_mode || "never",
    "bookmark_bar_mode",
  );

  const newTabMode = choice(
    [
      ["inherit", "As everywhere else"],
      ["always", "Always show"],
      ["never", "Always hidden"],
    ],
    settings.bookmark_bar_newtab ? "always" : "inherit",
    (value) => save({ bookmark_bar_newtab: value === "always" }),
  );

  card.append(
    row(t("Show the folders in columns"), t("Each column is this wide, in pixels"), columnsRow),
    revealRow,
    row(
      t("New bookmarks go to the top"),
      "",
      switchEl(Boolean(settings.bookmark_newest_first), (on) => save({ bookmark_newest_first: on })),
    ),
    row(
      t("Show icons without their names"),
      "",
      switchEl(Boolean(settings.bookmark_icons_only), (on) => save({ bookmark_icons_only: on })),
    ),
    row(
      t("Always show the bar on the new tab page"),
      "",
      switchEl(Boolean(settings.bookmark_bar_newtab), (on) => save({ bookmark_bar_newtab: on })),
    ),
    row(
      t("A middle click on a folder opens all of it"),
      "",
      switchEl(
        settings.bookmark_folder_middle_click !== false,
        (on) => save({ bookmark_folder_middle_click: on }),
      ),
    ),
    row(t("Widest a bar button may be"), t("In pixels; longer names are cut to fit"), width.holder),
    row(
      t("Show the bar"),
      "",
      (() => {
        const wrap = document.createElement("div");
        wrap.className = "row-inline";
        wrap.append(barMode, newTabMode);
        return wrap;
      })(),
    ),
  );

  return [head, card];
}
