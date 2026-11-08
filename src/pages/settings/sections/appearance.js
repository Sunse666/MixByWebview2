// Appearance: the palette, the theme file, page zoom and the font controls.
import { t } from "../../../i18n.js";
import { row } from "../ui.js";
import { SIZE_PRESETS, MIN_SIZES, ZOOM_LEVELS } from "../tables.js";

export function buildAppearance({ settings, invoke }) {
  // --- zoom ---------------------------------------------------------------
  const zoomSelect = document.createElement("select");
  zoomSelect.className = "button";
  for (const level of ZOOM_LEVELS) {
    const option = document.createElement("option");
    option.value = String(level);
    option.textContent = Math.round(level * 100) + "%";
    if (Math.abs((settings.default_zoom ?? 1) - level) < 0.01) option.selected = true;
    zoomSelect.append(option);
  }
  zoomSelect.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { default_zoom: Number(zoomSelect.value) },
    }).catch(() => {});
  });
  const zoomRow = row("Page zoom", "Applied to newly opened pages", zoomSelect);

  // --- fonts --------------------------------------------------------------
  const fontInput = document.createElement("input");
  fontInput.className = "hex";
  fontInput.style.width = "240px";
  fontInput.placeholder = "Microsoft YaHei";
  fontInput.value = settings.font_family || "";
  fontInput.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_family: fontInput.value } }).catch(() => {});
  });

  // Size presets, like the appearance pages of the browsers this imitates:
  // 0 keeps whatever the site asked for.
  const sizeSelect = document.createElement("select");
  sizeSelect.className = "button";
  for (const [size, label] of SIZE_PRESETS) {
    const option = document.createElement("option");
    option.value = String(size);
    option.textContent = size === 0 ? t(label) : `${t(label)} (${size}px)`;
    if ((settings.font_size ?? 0) === size) option.selected = true;
    sizeSelect.append(option);
  }
  sizeSelect.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_size: Number(sizeSelect.value) } }).catch(() => {});
  });

  const minSelect = document.createElement("select");
  minSelect.className = "button";
  for (const size of MIN_SIZES) {
    const option = document.createElement("option");
    option.value = String(size);
    option.textContent = size === 0 ? t("Off") : size + " px";
    if ((settings.font_min_size ?? 0) === size) option.selected = true;
    minSelect.append(option);
  }
  minSelect.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { font_min_size: Number(minSelect.value) },
    }).catch(() => {});
  });

  const monoInput = document.createElement("input");
  monoInput.className = "hex";
  monoInput.style.width = "240px";
  monoInput.placeholder = "JetBrains Mono";
  monoInput.value = settings.font_mono || "";
  monoInput.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { font_mono: monoInput.value } }).catch(() => {});
  });

  // Live preview: the same sample line drawn twice, once in each family.
  const preview = document.createElement("div");
  preview.style.padding = "10px 0 2px";
  const previewText = document.createElement("div");
  previewText.textContent = "16: The quick brown fox jumps over the lazy dog";
  previewText.style.fontSize = "16px";
  const previewMono = document.createElement("div");
  previewMono.textContent = "13: const mix = await open(url);";
  previewMono.style.fontSize = "13px";
  preview.append(previewText, previewMono);
  const paintPreview = () => {
    previewText.style.fontFamily = fontInput.value.trim() || "";
    previewMono.style.fontFamily =
      monoInput.value.trim() || "ui-monospace, Courier New, monospace";
  };
  fontInput.addEventListener("input", paintPreview);
  monoInput.addEventListener("input", paintPreview);
  paintPreview();

  // --- custom stylesheet --------------------------------------------------
  const cssButton = document.createElement("button");
  cssButton.className = "button";
  cssButton.textContent = t("Edit custom CSS");
  const cssNote = document.createElement("span");
  cssNote.className = "card-note";
  cssButton.addEventListener("click", async () => {
    const path = await invoke("custom_css_open").catch((error) => "error: " + String(error));
    cssNote.textContent = String(path);
  });
  const cssRow = document.createElement("div");
  cssRow.className = "card-actions";
  cssRow.append(cssButton, cssNote);
  const cssRowTitle = row(
    "Custom CSS",
    "UserData/custom.css is applied to every page; restart to reload",
  );

  // --- the section --------------------------------------------------------
  const themeHead = document.createElement("h2");
  themeHead.textContent = t("Appearance");
  themeHead.style.margin = "18px 0 10px";

  const themeCard = document.createElement("div");
  themeCard.className = "card";
  themeCard.style.maxWidth = "720px";
  const themeNote = document.createElement("span");
  themeNote.className = "card-note";

  const themeExport = document.createElement("button");
  themeExport.className = "button";
  themeExport.textContent = t("Export theme");
  themeExport.addEventListener("click", async () => {
    const path = await invoke("theme_export").catch((error) => "error: " + String(error));
    themeNote.textContent = String(path);
  });

  const themeImport = document.createElement("button");
  themeImport.className = "button";
  themeImport.textContent = t("Import theme");
  themeImport.addEventListener("click", async () => {
    const result = await invoke("theme_import").catch((error) => "error: " + String(error));
    themeNote.textContent = t("Theme loaded") + ": " + String(result);
  });

  const themeRow = document.createElement("div");
  themeRow.className = "card-actions";
  themeRow.append(themeExport, themeImport, themeNote);
  const themeText = document.createElement("div");
  themeText.className = "card-note";
  themeText.textContent = t("theme.json can be shared with the same folder layout");
  const modeSelect = document.createElement("select");
  modeSelect.className = "button";
  for (const [value, label] of [["dark", "Dark"], ["light", "Light"], ["system", "Follow system"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.theme_mode || "dark") === value) option.selected = true;
    modeSelect.append(option);
  }
  modeSelect.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { theme_mode: modeSelect.value } }).catch(() => {});
    location.reload();
  });

  // The sidebar lives next to the pages, so its switch belongs with the interface
  // rather than with the tab behaviour.
  const sideSelect = document.createElement("select");
  sideSelect.className = "button";
  for (const [value, label] of [
    ["off", "Off"],
    ["left", "Left"],
    ["right", "Right"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.sidebar || "off") === value) option.selected = true;
    sideSelect.append(option);
  }
  sideSelect.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { sidebar: sideSelect.value } }).catch(() => {});
  });

  themeCard.append(
    themeText,
    row("Mode", "Dark or light palette for the interface", modeSelect),
    row(t("Sidebar position"), t("Bookmarks, downloads and history in a column beside the page"), sideSelect),
    themeRow,
    zoomRow,
    row(t("Standard font"), t("Empty keeps the site font, applies to new pages"), fontInput),
    row(t("Monospace font"), t("Used for code blocks and text areas"), monoInput),
    row(t("Font size"), t("Text size for pages that do not set one"), sizeSelect),
    row(t("Minimum font size"), t("Stops pages from rendering text smaller than this"), minSelect),
    preview,
    cssRowTitle,
    cssRow,
  );

  return [themeHead, themeCard];
}
