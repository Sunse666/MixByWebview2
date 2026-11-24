// Browsing: the habits that are not about a particular surface.
//
// The rows follow the browser this one imitates, in its order: what the mouse does, what the
// window says when it takes the whole screen, how much the system is allowed to decide, and
// finally which programs a page may hand a link to.
import { t } from "../../../i18n.js";
import { row, switchEl } from "../ui.js";

export function buildBrowsing({ settings, invoke }) {
  const save = (patch) => invoke("settings_set", { patch }).catch(() => {});

  const head = document.createElement("h2");
  head.textContent = t("Browsing");
  head.style.margin = "18px 0 10px";

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  /// A switch bound to one boolean setting.
  const switchRow = (label, key) =>
    row(label, "", switchEl(Boolean(settings[key]), (on) => save({ [key]: on })));

  // --- eye comfort --------------------------------------------------------
  // A colour picker beside the switch rather than in its own row: the tint and whether it is
  // applied are one decision, and a page tinted with a colour nobody chose is worse than no
  // tint at all. The swatch is a native colour input, so it opens the system picker.
  const tint = document.createElement("input");
  tint.type = "color";
  tint.className = "swatch";
  tint.value = settings.eye_comfort_color || "#C7EDCC";
  tint.addEventListener("change", () => save({ eye_comfort_color: tint.value }));

  const eyeBox = document.createElement("input");
  eyeBox.type = "checkbox";
  eyeBox.checked = Boolean(settings.eye_comfort);
  eyeBox.addEventListener("change", async () => {
    tint.disabled = !eyeBox.checked;
    await save({ eye_comfort: eyeBox.checked });
  });
  tint.disabled = !eyeBox.checked;

  const eyeLabel = document.createElement("span");
  eyeLabel.textContent = t("Tint the pages");
  const eyeRow = document.createElement("div");
  eyeRow.className = "row-inline";
  eyeRow.append(eyeBox, eyeLabel, tint);

  // --- external protocols ------------------------------------------------
  // One field, because the list is long and read as one sentence: the schemes a page may
  // pass to the system without a prompt. Everything not named here asks first, which is the
  // safe direction - a site cannot launch a program by naming a scheme nobody listed.
  const schemes = document.createElement("input");
  schemes.className = "hex";
  schemes.style.width = "100%";
  schemes.placeholder = "magnet, ed2k, thunder, …";
  schemes.value = settings.external_schemes || "";
  schemes.addEventListener("change", () =>
    save({ external_schemes: schemes.value.trim() }),
  );
  const schemesBlock = document.createElement("div");
  schemesBlock.className = "field-block";
  schemesBlock.append(schemes);

  card.append(
    switchRow("Autoscroll with the middle button", "autoscroll"),
    // Not offered as a switch. Pasting on a middle click needs the clipboard read from inside
    // the page, and the command that reads it is deliberately reachable only from the app's
    // own pages: a page that could read the clipboard could read it whenever it liked, not
    // only when the reader pressed something. The row says what it would take instead of
    // offering a switch that would do nothing.
    row(
      t("A middle click pastes"),
      t("Not available: a page cannot read the clipboard without asking"),
      (() => {
        const note = document.createElement("span");
        note.className = "card-note";
        note.textContent = t("Not available");
        return note;
      })(),
    ),
    switchRow("Copy the selection as soon as it is made", "copy_on_select"),
    switchRow("Say so when a page goes fullscreen", "fullscreen_hint"),
    switchRow("Say so when a video opens in its own window", "video_popup_hint"),
    switchRow("Put that window back where it was left", "remember_popup_position"),
    row(
      t("Draw everything at 96 dpi"),
      t("Ignores the display's scaling; applies after a restart"),
      switchEl(Boolean(settings.ignore_system_dpi), (on) => save({ ignore_system_dpi: on })),
    ),
    eyeRow,
    row(
      t("Schemes a page may open without asking"),
      t("Separated by a comma, a semicolon or a space"),
    ),
    schemesBlock,
  );

  return [head, card];
}
