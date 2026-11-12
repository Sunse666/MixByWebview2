// Toolbar buttons: which ones show and in what order.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";
import { TOOLBAR_CHOICES, DEFAULT_TOOLBAR } from "../tables.js";

export function buildToolbar({ settings, invoke }) {
  const toolbarHead = document.createElement("h2");
  toolbarHead.textContent = t("Toolbar");
  toolbarHead.style.margin = "18px 0 10px";

  const toolbarCard = document.createElement("div");
  toolbarCard.className = "card";
  toolbarCard.style.maxWidth = "720px";

  const toolbarIds = () =>
    Array.isArray(settings.toolbar) && settings.toolbar.length
      ? [...settings.toolbar]
      : [...DEFAULT_TOOLBAR];
  const saveToolbar = async (next) => {
    settings.toolbar = next;
    await invoke("settings_set", { patch: { toolbar: next } }).catch(() => {});
  };

  for (const [id, label] of TOOLBAR_CHOICES) {
    toolbarCard.append(
      row(
        t(label),
        "",
        toggle(toolbarIds().includes(id), async (on) => {
          const list = toolbarIds().filter((item) => item !== id);
          if (on) list.push(id);
          await saveToolbar(list);
        }),
      ),
    );
  }

  const bookmarkPlace = document.createElement("select");
  bookmarkPlace.className = "button";
  for (const [value, label] of [["left", "Left"], ["right", "Right"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((value === "left") === (toolbarIds()[0] === "bookmark")) option.selected = true;
    bookmarkPlace.append(option);
  }
  bookmarkPlace.addEventListener("change", async () => {
    const list = toolbarIds().filter((item) => item !== "bookmark");
    if (bookmarkPlace.value === "left") list.unshift("bookmark");
    else list.push("bookmark");
    await saveToolbar(list);
  });
  toolbarCard.append(row(t("Bookmark button position"), "", bookmarkPlace));

  const toolbarReset = document.createElement("button");
  toolbarReset.className = "button";
  toolbarReset.textContent = t("Reset to defaults");
  toolbarReset.addEventListener("click", () => saveToolbar([...DEFAULT_TOOLBAR]));
  const resetRow = document.createElement("div");
  resetRow.className = "card-actions";
  resetRow.append(toolbarReset);
  toolbarCard.append(resetRow);

  return [toolbarHead, toolbarCard];
}
