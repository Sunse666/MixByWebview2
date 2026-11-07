// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildClearData({ settings, invoke }) {  const dataHead = document.createElement("h2");
  dataHead.textContent = t("Clear browsing data");
  dataHead.style.margin = "18px 0 10px";

  const dataCard = document.createElement("div");
  dataCard.className = "card";
  dataCard.style.maxWidth = "720px";

  const KINDS = {
    history: "History",
    bookmarks: "Bookmarks",
    favicons: "Site icons",
    zoom: "Zoom levels",
    session: "Session",
  };
  const picked = new Set(["history"]);

  for (const [kind, label] of Object.entries(KINDS)) {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "pick";
    box.checked = picked.has(kind);
    box.addEventListener("change", () => {
      if (box.checked) picked.add(kind);
      else picked.delete(kind);
    });
    const line = document.createElement("div");
    line.className = "entry";
    const text = document.createElement("span");
    text.className = "title";
    text.textContent = t(label);
    line.append(box, text);
    dataCard.append(line);
  }

  const run = document.createElement("button");
  run.className = "button";
  run.textContent = t("Clear selected data");
  run.addEventListener("click", async () => {
    // Irreversible: history, bookmarks, session and zoom have no undo.
    if (!window.confirm(t("Clear the selected data? This cannot be undone."))) return;
    const done = await invoke("clear_data", { kinds: [...picked] }).catch(() => []);
    run.textContent = t("Cleared") + ": " + (done || []).join(", ");
    setTimeout(() => {
      run.textContent = t("Clear selected data");
    }, 2500);
  });
  const runRow = document.createElement("div");
  runRow.className = "card-actions";
  runRow.append(run);
  dataCard.append(runRow);

  return [dataHead, dataCard];
}
