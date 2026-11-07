// Super drag: the switches, the threshold and one row per direction.
import { t } from "../../../i18n.js";
import { row, switchEl } from "../ui.js";
import { DRAG_ACTIONS, DRAG_DEFAULTS } from "../tables.js";

export function buildDrag({ settings, invoke }) {
  const dragHead = document.createElement("h2");
  dragHead.textContent = t("Super drag");
  dragHead.style.margin = "18px 0 10px";

  const dragCard = document.createElement("div");
  dragCard.className = "card";
  dragCard.style.maxWidth = "720px";
  const dragSwitch = switchEl(settings.super_drag !== false, async (next) => {
    await invoke("settings_set", { patch: { super_drag: next } }).catch(() => {});
  });
  const hintSwitch = switchEl(settings.drag_hint !== false, async (next) => {
    await invoke("settings_set", { patch: { drag_hint: next } }).catch(() => {});
  });

  const dragThreshold = document.createElement("input");
  dragThreshold.type = "range";
  dragThreshold.min = "8";
  dragThreshold.max = "120";
  dragThreshold.value = String(settings.drag_threshold ?? 28);
  dragThreshold.style.width = "220px";
  const dragValue = document.createElement("span");
  dragValue.className = "card-note";
  dragValue.textContent = dragThreshold.value;
  dragThreshold.addEventListener("input", () => {
    dragValue.textContent = dragThreshold.value;
  });
  dragThreshold.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { drag_threshold: Number(dragThreshold.value) },
    }).catch(() => {});
  });
  const dragHolder = document.createElement("div");
  dragHolder.style.display = "flex";
  dragHolder.style.gap = "10px";
  dragHolder.style.alignItems = "center";
  dragHolder.append(dragThreshold, dragValue);

  dragCard.append(
    row(
      t("Enable super drag"),
      t("Drag a link to open it, drag selected text to search it"),
      dragSwitch,
    ),
    row(t("Show the drag badge"), "", hintSwitch),
    row(t("Threshold"), "", dragHolder),
  );

  const dragMap = { ...DRAG_DEFAULTS, ...(settings.drag_map || {}) };
  const dragGrid = document.createElement("div");
  dragGrid.style.display = "grid";
  dragGrid.style.gridTemplateColumns = "repeat(2, 1fr)";
  dragGrid.style.gap = "10px";
  dragGrid.style.padding = "14px 18px";

  for (const [dir, glyph] of [["U", "\u2191"], ["D", "\u2193"], ["L", "\u2190"], ["R", "\u2192"]]) {
    const cell = document.createElement("div");
    cell.style.display = "flex";
    cell.style.alignItems = "center";
    cell.style.gap = "8px";
    const drawn = document.createElement("span");
    drawn.textContent = glyph;
    drawn.style.width = "30px";
    drawn.style.color = "#8ab4f8";
    const select = document.createElement("select");
    select.className = "button";
    select.style.flex = "1";
    for (const [id, label] of Object.entries(DRAG_ACTIONS)) {
      const option = document.createElement("option");
      option.value = id;
      option.textContent = t(label);
      if ((dragMap[dir] || "none") === id) option.selected = true;
      select.append(option);
    }
    select.addEventListener("change", async () => {
      await invoke("settings_set", {
        patch: { drag_map: { ...dragMap, [dir]: select.value } },
      }).catch(() => {});
    });
    cell.append(drawn, select);
    dragGrid.append(cell);
  }
  dragCard.append(dragGrid);

  return [dragHead, dragCard];
}
