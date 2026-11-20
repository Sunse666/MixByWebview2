// The article this tab was opened from, without the page around it.
//
// The extraction ran in the page that had the document; this only lays it out. The hash
// carries which tab to ask, and the markup comes back through a command.
const { invoke } = window.__TAURI__.core;

const title = document.getElementById("title");
const byline = document.getElementById("byline");
const body = document.getElementById("body");
const src = document.getElementById("src");

invoke("article_get")
  .then((article) => {
    if (!article || !article.html) {
      body.innerHTML = "";
      const empty = document.createElement("p");
      empty.className = "empty";
      empty.textContent = "No article could be found on that page.";
      body.append(empty);
      return;
    }
    title.textContent = article.title || "";
    byline.textContent = article.byline || "";
    // The extracted markup is the article, so it is inserted as markup: escaping it
    // would print the tags instead of the story. It came from the page over a command
    // only an app page may answer.
    body.innerHTML = article.html;
    src.textContent = article.source || "";
    if (article.title) document.title = article.title;
  })
  .catch((error) => {
    body.textContent = "Could not read the article: " + String(error);
  });
