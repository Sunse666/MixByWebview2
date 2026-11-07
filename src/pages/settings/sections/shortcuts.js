// Keyboard shortcuts: one field per action, with a duplicate warning.
import { t } from "../../../i18n.js";
import { row } from "../ui.js";
import { SHORTCUT_ACTIONS, SHORTCUT_DEFAULTS } from "../tables.js";

export function buildShortcuts({ settings, invoke }) {
  const keysHead = document.createElement("h2");
  keysHead.textContent = t("Shortcuts");
  keysHead.style.margin = "18px 0 10px";

  const keysCard = document.createElement("div");
  keysCard.className = "card";
  keysCard.style.maxWidth = "720px";

  const current = { ...SHORTCUT_DEFAULTS, ...(settings.shortcuts || {}) };
  const saved = { ...(settings.shortcuts || {}) };

  const conflict = document.createElement("div");
  conflict.className = "card-note";

  const checkConflicts = () => {
    const seen = new Map();
    const dupes = new Set();
    for (const [action, combo] of Object.entries(current)) {
      if (!combo) continue;
      if (seen.has(combo)) dupes.add(combo);
      seen.set(combo, action);
    }
    conflict.textContent = dupes.size
      ? t("These combos are used twice") + ": " + [...dupes].join(", ")
      : "";
  };

  for (const [action, label] of Object.entries(SHORTCUT_ACTIONS)) {
    const input = document.createElement("input");
    input.className = "hex";
    input.style.width = "160px";
    input.value = current[action] || "";
    input.placeholder = SHORTCUT_DEFAULTS[action] || "";
    input.addEventListener("change", async () => {
      const combo = input.value.trim().toLowerCase();
      current[action] = combo;
      if (combo && combo !== SHORTCUT_DEFAULTS[action]) saved[action] = combo;
      else delete saved[action];
      checkConflicts();
      await invoke("settings_set", { patch: { shortcuts: saved } }).catch(() => {});
    });
    keysCard.append(row(t(label), SHORTCUT_DEFAULTS[action] || "", input));
  }

  keysCard.append(conflict);
  checkConflicts();

  return [keysHead, keysCard];
}
