// Shared widgets for the settings page.
import { t } from "../../i18n.js";

const { invoke } = window.__TAURI__.core;

export function navItem(nav, glyph, label, active, page) {
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

export function row(title, detail, control) {
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

export function toggle(on, onChange) {
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

/** Shared pill switch, so no row falls back to a bare checkbox. */
export function switchEl(checked, onChange) {
  const button = document.createElement("button");
  button.className = "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(Boolean(checked)));
  button.addEventListener("click", () => {
    const next = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(next));
    onChange(next);
  });
  return button;
}

export function themePreview() {
  const holder = document.createElement("div");
  holder.className = "card-actions";
  holder.append(editButton(holder));
  return holder;
}

export function editButton(holder) {
  const edit = document.createElement("button");
  edit.className = "button";
  edit.textContent = t("Edit");
  edit.addEventListener("click", () => openThemeEditor(holder));
  return edit;
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
    holder.replaceChildren();
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

  const preview = () => invoke("theme_apply", { theme: draft, persist: false }).catch(() => {});
  holder.addEventListener("input", preview);
}

/** A range input with its value shown next to it. */
export function slider(value, min, max, onCommit) {
  const input = document.createElement("input");
  input.type = "range";
  input.min = String(min);
  input.max = String(max);
  input.value = String(value);
  input.style.width = "220px";
  const shown = document.createElement("span");
  shown.className = "card-note";
  shown.textContent = String(value);
  input.addEventListener("input", () => {
    shown.textContent = input.value;
  });
  input.addEventListener("change", () => onCommit(Number(input.value), input));
  const holder = document.createElement("div");
  holder.style.display = "flex";
  holder.style.alignItems = "center";
  holder.style.gap = "10px";
  holder.append(input, shown);
  return { input, holder };
}

/** A text field that writes one settings key on change. */
export function textField(invoke, value, key, options = {}) {
  const input = document.createElement("input");
  input.className = "hex";
  input.style.width = options.width || "320px";
  if (options.placeholder) input.placeholder = options.placeholder;
  input.value = value || "";
  input.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { [key]: options.number ? Number(input.value) : input.value } }).catch(() => {});
  });
  return input;
}

/** A select over [value, label] pairs that writes one settings key. */
export function selectField(invoke, options, value, key, onAfter) {
  const select = document.createElement("select");
  select.className = "button";
  for (const [optionValue, label] of options) {
    const option = document.createElement("option");
    option.value = String(optionValue);
    option.textContent = typeof label === "function" ? label(optionValue) : t(label);
    if (String(value) === String(optionValue)) option.selected = true;
    select.append(option);
  }
  select.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { [key]: select.value } }).catch(() => {});
    if (onAfter) onAfter(select.value);
  });
  return select;
}
