// Mouse gestures: the enable switches, the trail, and one row per pattern.
import { t } from "../../../i18n.js";
import { row, switchEl } from "../ui.js";
import { GESTURE_ACTIONS, PATTERNS, DEFAULTS_GESTURE } from "../tables.js";

export function buildGestures({ settings, invoke }) {
  const tabToggle = (label, note, value, key) => {
    const control = switchEl(value !== false, async (next) => {
      await invoke("settings_set", { patch: { [key]: next } }).catch(() => {});
    });
    return row(t(label), t(note), control);
  };

  const gestureMap = { ...DEFAULTS_GESTURE, ...(settings.gesture_map || {}) };

  const gestureCard = document.createElement("div");
  gestureCard.className = "card";
  gestureCard.style.maxWidth = "720px";

  gestureCard.append(
    tabToggle("Enable mouse gestures", "Right button drag", settings.gestures, "gestures"),
    tabToggle("Show the trail", "Draw the path while dragging", settings.gesture_trail, "gesture_trail"),
  );

  const color = document.createElement("input");
  color.type = "color";
  color.value = settings.gesture_color || "#4c8dff";
  color.style.width = "64px";
  color.style.height = "28px";
  color.style.border = "1px solid #3c4043";
  color.style.borderRadius = "6px";
  color.style.background = "transparent";
  color.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_color: color.value } }).catch(() => {});
  });
  gestureCard.append(row(t("Trail colour"), "", color));

  const width = document.createElement("input");
  width.type = "range";
  width.min = "1";
  width.max = "10";
  width.value = String(settings.gesture_width ?? 3);
  width.style.width = "220px";
  const widthValue = document.createElement("span");
  widthValue.className = "card-note";
  widthValue.textContent = width.value;
  width.addEventListener("input", () => {
    widthValue.textContent = width.value;
  });
  width.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_width: Number(width.value) } }).catch(() => {});
  });
  const widthHolder = document.createElement("div");
  widthHolder.style.display = "flex";
  widthHolder.style.gap = "10px";
  widthHolder.style.alignItems = "center";
  widthHolder.append(width, widthValue);
  gestureCard.append(row(t("Trail width"), "", widthHolder));

  const threshold = document.createElement("input");
  threshold.type = "range";
  threshold.min = "10";
  threshold.max = "120";
  threshold.value = String(settings.gesture_threshold ?? 34);
  threshold.style.width = "220px";
  const thresholdValue = document.createElement("span");
  thresholdValue.className = "card-note";
  thresholdValue.textContent = threshold.value;
  threshold.addEventListener("input", () => {
    thresholdValue.textContent = threshold.value;
  });
  threshold.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { gesture_threshold: Number(threshold.value) },
    }).catch(() => {});
  });
  const thresholdHolder = document.createElement("div");
  thresholdHolder.style.display = "flex";
  thresholdHolder.style.gap = "10px";
  thresholdHolder.style.alignItems = "center";
  thresholdHolder.append(threshold, thresholdValue);
  gestureCard.append(row(t("Threshold"), "Pixels before a direction counts", thresholdHolder));

  const grid = document.createElement("div");
  grid.style.display = "grid";
  grid.style.gridTemplateColumns = "repeat(2, 1fr)";
  grid.style.gap = "10px";
  grid.style.padding = "14px 18px";

  for (const [pattern, glyph] of PATTERNS) {
    const cell = document.createElement("div");
    cell.style.display = "flex";
    cell.style.alignItems = "center";
    cell.style.gap = "8px";

    const drawn = document.createElement("span");
    drawn.textContent = glyph;
    drawn.style.width = "30px";
    drawn.style.color = "#8ab4f8";
    drawn.style.fontSize = "13px";

    const select = document.createElement("select");
    select.className = "button";
    select.style.flex = "1";
    for (const [id, label] of Object.entries(GESTURE_ACTIONS)) {
      const option = document.createElement("option");
      option.value = id;
      option.textContent = t(label);
      if ((gestureMap[pattern] || "none") === id) option.selected = true;
      select.append(option);
    }
    select.addEventListener("change", async () => {
      await invoke("settings_set", {
        patch: { gesture_map: { ...gestureMap, [pattern]: select.value } },
      }).catch(() => {});
    });

    cell.append(drawn, select);
    grid.append(cell);
  }
  gestureCard.append(grid);

  const blocked = document.createElement("input");
  blocked.className = "hex";
  blocked.style.width = "420px";
  blocked.placeholder = "example.com, another.com";
  blocked.value = settings.gesture_blocked || "";
  blocked.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { gesture_blocked: blocked.value } }).catch(() => {});
  });
  gestureCard.append(row(t("Disable on these hosts"), "Comma separated", blocked));

  const gesturesHead = document.createElement("h2");
  gesturesHead.textContent = t("Mouse gestures");
  gesturesHead.style.margin = "18px 0 10px";

  return [gesturesHead, gestureCard];
}
