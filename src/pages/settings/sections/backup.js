// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildBackup({ settings, invoke }) {  const backupHead = document.createElement("h2");
  backupHead.textContent = t("Backup");
  backupHead.style.margin = "18px 0 10px";

  const backupCard = document.createElement("div");
  backupCard.className = "card";
  backupCard.style.maxWidth = "720px";

  const backupNote = document.createElement("span");
  backupNote.className = "card-note";

  const exportButton = document.createElement("button");
  exportButton.className = "button";
  exportButton.textContent = t("Export profile");
  exportButton.addEventListener("click", async () => {
    const path = await invoke("backup_export").catch((error) => "error: " + String(error));
    backupNote.textContent = String(path);
  });

  const importButton = document.createElement("button");
  importButton.className = "button";
  importButton.textContent = t("Import profile");
  importButton.addEventListener("click", async () => {
    const result = await invoke("backup_import").catch((error) => "error: " + String(error));
    backupNote.textContent = t("Restored") + ": " + String(result) + " — " + t("restart to apply");
  });

  const backupRow = document.createElement("div");
  backupRow.className = "card-actions";
  backupRow.append(exportButton, importButton, backupNote);
  const backupText = document.createElement("div");
  backupText.className = "card-note";
  backupText.textContent = t(
    "Settings, history, bookmarks, theme and icons without the WebView2 profile",
  );
  backupCard.append(backupText, backupRow);

  return [backupHead, backupCard];
}
