// The markup of the page this tab was opened from.
//
// It arrives through a command rather than in the url: a data url is refused by the
// webview, and a whole document does not fit in a hash.
const { invoke } = window.__TAURI__.core;

const out = document.getElementById("code");

invoke("source_get")
  .then((html) => {
    out.textContent = html || "";
  })
  .catch((error) => {
    out.textContent = "Could not read the source: " + String(error);
  });
