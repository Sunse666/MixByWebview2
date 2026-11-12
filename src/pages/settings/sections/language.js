// Interface language.
import { setLanguage, t } from "../../../i18n.js";
import { row } from "../ui.js";

export function buildLanguage({ settings, invoke }) {
  const language = document.createElement("select");
  language.className = "button";
  for (const [value, label] of [["en", "English"], ["zh", "Simplified Chinese"]]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.language || "en") === value) option.selected = true;
    language.append(option);
  }
  language.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { language: language.value } }).catch(() => {});
    setLanguage(language.value);
    location.reload();
  });

  const langHead = document.createElement("h2");
  langHead.textContent = t("Language");
  langHead.style.margin = "18px 0 10px";
  const langCard = document.createElement("div");
  langCard.className = "card";
  langCard.style.maxWidth = "720px";
  langCard.append(row("Interface language", "Choose the language of the interface", language));

  return [langHead, langCard];
}
