// Tab settings: what a click on a tab does, where a new one lands, and what the strip draws.
//
// The order is the screenshot this page restores, and it is also the order a reader thinks
// in: what a click does, then the strip's own width and buttons, then what each kind of
// opening does, then what closing does.
import { t } from "../../../i18n.js";
import { row, switchEl, slider } from "../ui.js";

export function buildTabs({ settings, invoke }) {
  const save = (patch) => invoke("settings_set", { patch }).catch(() => {});

  /// A select over [value, label] pairs whose write is the caller's.
  ///
  /// Not `selectField`: six of these store a string under one key, and building them all
  /// through one helper keeps the option lists beside the rows that use them.
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
    row(label, "", switchEl(Boolean(settings[key]), (on) => save({ [key]: on })));

  const head = document.createElement("h2");
  head.textContent = t("Tab settings");
  head.style.margin = "18px 0 10px";

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  // --- the shape of a tab ------------------------------------------------
  // Two little drawings rather than the words "rounded" and "square": the difference is a
  // corner, and a corner is easier to see than to name. Each is a radio plus the outline
  // it stands for, so the pair reads as one choice.
  const shapeWrap = document.createElement("div");
  shapeWrap.className = "shape-choice";
  const shapes = [];
  for (const [value, label] of [
    ["rounded", "Rounded corners"],
    ["square", "Square corners"],
  ]) {
    const option = document.createElement("label");
    option.className = "shape-option";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "tab-shape";
    radio.value = value;
    radio.checked = (settings.tab_shape || "rounded") === value;
    radio.addEventListener("change", () => {
      if (radio.checked) save({ tab_shape: value });
    });
    const sketch = document.createElement("span");
    sketch.className = `shape-sketch ${value}`;
    option.title = t(label);
    option.append(radio, sketch);
    shapeWrap.append(option);
    shapes.push(radio);
  }

  // --- the strip's own numbers -------------------------------------------
  const minWidth = slider(
    Math.round(settings.tab_min_width ?? 120),
    60,
    260,
    (value) => save({ tab_min_width: value }),
  );
  const minWidthRow = document.createElement("div");
  minWidthRow.className = "row-inline";
  const minWidthBox = document.createElement("input");
  minWidthBox.type = "checkbox";
  minWidthBox.checked = true;
  minWidthRow.append(minWidthBox, minWidth.input, minWidth.value);

  const scrollSpeed = slider(
    Number(settings.tab_scroll_speed ?? 3),
    1,
    12,
    (value) => save({ tab_scroll_speed: value }),
  );

  // One number inside a sentence, for the same reason the bookmarks page words its reveal
  // that way: a wait is not a quantity a reader wants a labelled field for.
  const hoverField = document.createElement("input");
  hoverField.className = "hex num";
  hoverField.type = "number";
  hoverField.min = "0";
  hoverField.max = "2000";
  hoverField.value = String(settings.tab_hover_activate ?? 0);
  hoverField.addEventListener("change", () =>
    save({ tab_hover_activate: Number(hoverField.value) || 0 }),
  );
  const hoverRow = document.createElement("div");
  hoverRow.className = "sentence";
  const hoverBox = document.createElement("input");
  hoverBox.type = "checkbox";
  hoverBox.checked = (settings.tab_hover_activate ?? 0) > 0;
  hoverBox.addEventListener("change", async () => {
    if (hoverBox.checked && !Number(hoverField.value)) hoverField.value = "200";
    if (!hoverBox.checked) hoverField.value = "0";
    hoverField.disabled = !hoverBox.checked;
    await save({ tab_hover_activate: Number(hoverField.value) || 0 });
  });
  hoverField.disabled = !hoverBox.checked;
  const sayHover = (text) => {
    const span = document.createElement("span");
    span.textContent = t(text);
    return span;
  };
  hoverRow.append(hoverBox, sayHover("Resting on a tab for"), hoverField, sayHover("ms activates it"));

  // --- what each kind of opening does ------------------------------------
  const openTargets = [
    ["foreground", "A foreground tab"],
    ["background", "A background tab"],
    ["current", "The current tab"],
  ];
  const openRows = [
    ["Bookmarks open in", "open_bookmark"],
    ["The home page opens in", "open_home"],
    ["Search results open in", "open_search"],
  ].map(([label, key]) =>
    row(label, "", choice(openTargets, settings[key], (value) => save({ [key]: value }))),
  );
  const addressRow = row(
    t("A typed address opens in"),
    "",
    choice(
      [
        ["current", "The current tab"],
        ["foreground", "A foreground tab"],
        ["background", "A background tab"],
      ],
      settings.open_address,
      (value) => save({ open_address: value }),
    ),
  );

  card.append(
    switchRow("Double click closes a tab", "tab_double_click_close"),
    switchRow("A right click closes a tab", "tab_right_click_close"),
    switchRow("A new tab page opens links in itself", "tab_reuse_blank"),
    switchRow("Every link opens in the current tab", "tab_force_current"),
    switchRow("Every link opens in the background", "tab_force_background"),
    switchRow("A popup opens as a tab", "tab_from_popup"),
    // The switch says "hide", the setting says "show": one of the two has to be inverted,
    // and the row's wording is what a reader looks for.
    row(
      t("Hide the close button"),
      "",
      switchEl(!settings.tab_show_close, (on) => save({ tab_show_close: !on })),
    ),
    switchRow("A pinned tab closes only from its menu", "tab_pinned_menu_only"),
    switchRow("A new tab copies the small-tab state", "tab_small_inherit"),
    switchRow("The wheel over the strip walks the tabs", "tab_wheel_switch"),
    switchRow("Ask before closing several tabs", "tab_close_warn"),
    row(t("Tab shape"), "", shapeWrap),
    row(t("Narrowest a tab may be"), t("In pixels"), minWidthRow),
    row(t("Show the all-tabs button"), "", switchEl(Boolean(settings.tab_list_button), (on) => save({ tab_list_button: on }))),
    row(t("Show the strip's scroll arrows"), "", switchEl(Boolean(settings.tab_scroll_buttons), (on) => save({ tab_scroll_buttons: on }))),
    hoverRow,
    row(t("Strip scroll speed"), t("Tabs per notch of the wheel"), scrollSpeed.holder),
    row(
      t("The speaker on a tab"),
      "",
      choice(
        [
          ["mute", "Mutes the tab"],
          ["activate", "Switches to the tab"],
        ],
        settings.tab_audio_click,
        (value) => save({ tab_audio_click: value }),
      ),
    ),
    ...openRows,
    addressRow,
    row(
      t("Where a new tab goes"),
      "",
      choice(
        [
          ["after_current", "Right of the current tab"],
          ["after_group", "Right of the current tab's group"],
          ["at_end", "At the end of the strip"],
        ],
        settings.tab_new_position,
        (value) => save({ tab_new_position: value }),
      ),
    ),
    row(
      t("After closing, switch to"),
      "",
      choice(
        [
          ["auto", "Pick for me"],
          ["left", "The tab on the left"],
          ["right", "The tab on the right"],
          ["last_used", "The tab used before this one"],
        ],
        settings.tab_close_to,
        (value) => save({ tab_close_to: value }),
      ),
    ),
    row(
      t("Closing the last tab"),
      "",
      choice(
        [
          ["new_tab", "Opens a new tab"],
          ["close_window", "Closes the window"],
          ["nothing", "Does nothing"],
        ],
        settings.tab_last_close,
        (value) => save({ tab_last_close: value }),
      ),
    ),
    row(
      t("A middle click on a tab"),
      "",
      choice(
        [
          ["close", "Closes it"],
          ["reload", "Reloads it"],
          ["nothing", "Does nothing"],
        ],
        settings.tab_middle_click,
        (value) => save({ tab_middle_click: value }),
      ),
    ),
    row(
      t("A middle click on the empty strip"),
      "",
      choice(
        [
          ["new_tab", "Opens a new tab"],
          ["close_others", "Closes the other tabs"],
          ["nothing", "Does nothing"],
        ],
        settings.tab_middle_strip,
        (value) => save({ tab_middle_strip: value }),
      ),
    ),
  );

  return [head, card];
}
