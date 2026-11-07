// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildNewtab({ settings, invoke }) {
  const newtabHead = document.createElement("h2");
  newtabHead.textContent = t("New tab page");
  newtabHead.style.margin = "18px 0 10px";

  const newtabCard = document.createElement("div");
  newtabCard.className = "card";
  newtabCard.style.maxWidth = "720px";

  const toggleRow = (label, value, key) => {
    const control = switchEl(value !== false, async (next) => {
      await invoke("settings_set", { patch: { [key]: next } }).catch(() => {});
    });
    return row(t(label), "", control);
  };

  newtabCard.append(
    toggleRow("Show search box", settings.newtab_show_search, "newtab_show_search"),
    toggleRow("Show frequent sites", settings.newtab_show_sites, "newtab_show_sites"),
  );

  const urlInput2 = document.createElement("input");
  urlInput2.className = "hex";
  urlInput2.style.width = "320px";
  urlInput2.placeholder = "https://example.com";
  urlInput2.value = settings.newtab_url || "";
  urlInput2.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { newtab_url: urlInput2.value } }).catch(() => {});
  });
  newtabCard.append(row(t("Custom address"), t("Opens instead of this page"), urlInput2));

  const slider = (label, value, key, min, max) => {
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
    input.addEventListener("change", async () => {
      await invoke("settings_set", { patch: { [key]: Number(input.value) } }).catch(() => {});
    });
    const holder = document.createElement("div");
    holder.style.display = "flex";
    holder.style.alignItems = "center";
    holder.style.gap = "10px";
    holder.append(input, shown);
    return row(t(label), "", holder);
  };

  newtabCard.append(
    slider("Max tiles", settings.newtab_tiles ?? 12, "newtab_tiles", 1, 40),
    slider("Max columns", settings.newtab_columns ?? 6, "newtab_columns", 1, 12),
    slider("Tile radius", settings.newtab_radius ?? 50, "newtab_radius", 0, 50),
  );

  return [newtabHead, newtabCard];
}
