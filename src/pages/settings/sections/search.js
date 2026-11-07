// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildSearch({ settings, invoke }) {
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
  const engineRow = row("Search engine", "Used when the address bar is not a url", engine);

  const customSearch = document.createElement("input");
  customSearch.className = "hex";
  customSearch.style.width = "320px";
  customSearch.placeholder = "https://example.com/search?q=%s";
  customSearch.value = settings.custom_search || "";
  customSearch.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { custom_search: customSearch.value } }).catch(() => {});
  });
  const customSearchRow = row(
    "Custom search url",
    "Overrides the engine above; %s is the query",
    customSearch,
  );

  const searchHead = document.createElement("h2");
  searchHead.textContent = t("Search engine");
  searchHead.style.margin = "18px 0 10px";
  const searchCard = document.createElement("div");
  searchCard.className = "card";
  searchCard.style.maxWidth = "720px";
  searchCard.append(engineRow, customSearchRow);

  return [searchHead, searchCard];
}
