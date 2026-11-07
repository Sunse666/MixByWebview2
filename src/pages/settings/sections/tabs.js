// Tab settings: what a click on a tab does.
import { t } from "../../../i18n.js";
import { row, switchEl } from "../ui.js";

export function buildTabs({ settings, invoke }) {
  const tabsHead = document.createElement("h2");
  tabsHead.textContent = t("Tab settings");
  tabsHead.style.margin = "18px 0 10px";

  const tabsCard = document.createElement("div");
  tabsCard.className = "card";
  tabsCard.style.maxWidth = "720px";

  const tabToggle = (label, note, value, key) => {
    const control = switchEl(value !== false, async (next) => {
      await invoke("settings_set", { patch: { [key]: next } }).catch(() => {});
    });
    return row(t(label), t(note), control);
  };

  // Off by default: the card costs a screenshot per tab switch.
  const preview = switchEl(settings.tab_hover_preview === true, async (next) => {
    await invoke("settings_set", { patch: { tab_hover_preview: next } }).catch(() => {});
  });

  tabsCard.append(
    tabToggle(
      "Double click closes a tab",
      "Off means a double click only selects it",
      settings.tab_double_click_close,
      "tab_double_click_close",
    ),
    tabToggle(
      "Show the close button",
      "Tabs can still be closed with Ctrl+W or the middle click",
      settings.tab_show_close,
      "tab_show_close",
    ),
    tabToggle(
      "Activate the tab on the left after closing",
      "Off selects the tab on the right",
      settings.tab_close_goes_left,
      "tab_close_goes_left",
    ),
    row(
      t("Preview on hover"),
      t("Shows the title and the address of a tab when the pointer rests on it"),
      preview,
    ),
  );

  return [tabsHead, tabsCard];
}
