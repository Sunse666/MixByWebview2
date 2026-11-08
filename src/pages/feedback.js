// A small confirmation for the pages that save as you go.
//
// Every setting is applied the moment it changes - there is no save button to
// press, which is the point - but the calls used to end in .catch(() => {}) so a
// failure looked exactly like a success, and nothing said anything either way.

const SHOWN_MS = 1400;

let holder = null;
let timer = null;

/** Shows a short message at the bottom of the page. */
export function toast(message, bad) {
  if (!holder) {
    holder = document.createElement("div");
    holder.className = "toast";
    document.body.append(holder);
  }
  holder.textContent = message;
  holder.classList.toggle("bad", Boolean(bad));
  holder.classList.add("on");
  clearTimeout(timer);
  timer = setTimeout(() => holder.classList.remove("on"), SHOWN_MS);
}

/** Commands worth confirming: the ones that write something down. */
const WRITES = new Set([
  "settings_set",
  "extension_live_set",
  "extension_add",
  "extension_set",
  "bookmark_toggle",
  "script_set_enabled",
  "history_remove",
  "bookmark_remove",
  "bookmark_update",
]);

/**
 * Wraps invoke so a write reports itself: a short "saved" on success, the reason on
 * failure. Failures are still swallowed afterwards, because the caller's own handler
 * has nothing useful to add once the message is on screen.
 */
export function withFeedback(run) {
  return (command, args) => {
    const pending = run(command, args);
    if (!WRITES.has(command) || !pending || !pending.then) {
      return pending;
    }
    return pending.then(
      (value) => {
        toast(window.__mixSavedText || "Saved");
        return value;
      },
      (error) => {
        toast((window.__mixFailedText || "Could not save:") + " " + String(error), true);
        return undefined;
      },
    );
  };
}

/** Lets a page supply its own words once the language is known. */
export function setFeedbackText(saved, failed) {
  window.__mixSavedText = saved;
  window.__mixFailedText = failed;
}
