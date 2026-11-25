  let __mixDismiss = false;
  window.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || __mixDismiss) return;
    __mixDismiss = true;
    setTimeout(() => { __mixDismiss = false; }, 100);
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: "dismiss" }).catch(() => {});
  }, true);

(function () {
  var sendIcon = function () {
    try {
      var link = document.querySelector('link[rel~="icon"]');
      var href = link && link.href ? link.href : "";
      if (!href && location.protocol.startsWith("http")) {
        href = location.origin + "/favicon.ico";
      }
      if (!href) return;
      window.__TAURI_INTERNALS__.invoke("tab_icon", { url: href }).catch(function () {});
    } catch (error) {
      /* ignore */
    }
  };
  document.addEventListener("DOMContentLoaded", sendIcon);
  setTimeout(sendIcon, 1500);
})();

// In-place page translation.
(function () {
  var nodes = [];
  var done = new WeakSet();
  var observer = null;
  var running = false;
  var timer = null;

  var documents = function () {
    var list = [document];
    var frames = document.querySelectorAll("iframe, frame");
    for (var i = 0; i < frames.length; i++) {
      try {
        var doc = frames[i].contentDocument;
        if (doc && doc.body) list.push(doc);
      } catch (error) {
        /* cross origin */
      }
    }
    return list;
  };

  var collect = function () {
    nodes = [];
    var seen = new Set();
    var total = 0;
    var docs = documents();

    for (var d = 0; d < docs.length; d++) {
      var walker = docs[d].createTreeWalker(docs[d].body, NodeFilter.SHOW_TEXT, {
        acceptNode: function (node) {
          var text = node.nodeValue || "";
          if (text.trim().length < 2) return NodeFilter.FILTER_REJECT;
          var parent = node.parentElement;
          if (!parent) return NodeFilter.FILTER_REJECT;
          if (/^(SCRIPT|STYLE|NOSCRIPT|TEXTAREA|CODE|PRE|KBD|SAMP)$/.test(parent.tagName)) {
            return NodeFilter.FILTER_REJECT;
          }
          return NodeFilter.FILTER_ACCEPT;
        },
      });

      while (walker.nextNode()) {
        var node = walker.currentNode;
        var text = node.nodeValue;
        if (done.has(node)) continue;
        if (seen.has(text)) continue;
        if (total + text.length > 6000) break;
        seen.add(text);
        total += text.length;
        nodes.push({ node: node, original: text });
        if (nodes.length >= 400) break;
      }
    }

    return nodes.map(function (item) {
      return item.original;
    });
  };

  var apply = function (translated) {
    var applied = 0;
    for (var i = 0; i < nodes.length; i++) {
      var replacement = translated[i];
      if (!replacement) continue;
      if (replacement === nodes[i].original) continue;
      nodes[i].node.nodeValue = replacement;
      done.add(nodes[i].node);
      applied++;
    }
    return applied;
  };

  var restore = function () {
    for (var i = 0; i < nodes.length; i++) {
      nodes[i].node.nodeValue = nodes[i].original;
    }
    nodes = [];
    done = new WeakSet();
  };

  var rounds = function (total) {
    var texts = collect();
    if (!texts.length) {
      window.__TAURI_INTERNALS__.invoke("translate_done", {
        count: total,
        error: total === 0 ? "no translatable text" : null,
      });
      running = false;
      return;
    }

    window.__TAURI_INTERNALS__
      .invoke("translate_texts", { texts: texts })
      .then(function (translated) {
        var applied = apply(translated || []);
        total += applied;
        if (applied === 0) {
          window.__TAURI_INTERNALS__.invoke("translate_done", {
            count: total,
            error: total === 0 ? "translation returned nothing" : null,
          });
          running = false;
          return;
        }
        setTimeout(function () {
          rounds(total);
        }, 50);
      })
      .catch(function (err) {
        window.__TAURI_INTERNALS__.invoke("translate_done", { count: total, error: String(err) });
        running = false;
      });
  };

  var watch = function () {
    if (observer) return;
    var notify = function () {
      if (timer) clearTimeout(timer);
      timer = setTimeout(function () {
        if (!running) window.__mixStartTranslate();
      }, 900);
    };
    observer = new MutationObserver(notify);
    var docs = documents();
    for (var i = 0; i < docs.length; i++) {
      observer.observe(docs[i].body, { childList: true, subtree: true });
    }
  };

  window.__mixStartTranslate = function () {
    if (running) return;
    running = true;
    rounds(0);
    watch();
  };

  window.__mixStopTranslate = function () {
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    if (timer) clearTimeout(timer);
    running = false;
    restore();
    window.__TAURI_INTERNALS__.invoke("translate_done", { count: 0 });
  };

  window.__mixTranslate = { collect: collect, apply: apply, restore: restore };
})();

// Fullscreen video.
(function () {
  var report = function (on) {
    window.__TAURI_INTERNALS__.invoke("page_fullscreen", { on: on }).catch(function () {});
  };

  document.addEventListener("fullscreenchange", function () {
    report(Boolean(document.fullscreenElement));
  });

  document.addEventListener("webkitfullscreenchange", function () {
    report(Boolean(document.webkitFullscreenElement));
  });
})();

// Picture-in-picture.
//
// The small window belongs to the platform, so a page can only say that it happened - which
// is all the browser needs to say something back. Both APIs are watched: the element one, and
// the document one a newer engine offers.
(function () {
  var tauri = window.__TAURI_INTERNALS__;
  if (!tauri || !tauri.invoke) return;
  var send = function () {
    tauri.invoke("video_pip", {}).catch(function () {});
  };
  document.addEventListener("enterpictureinpicture", send);
  if ("documentPictureInPicture" in window && window.documentPictureInPicture) {
    window.documentPictureInPicture.addEventListener("enter", send);
  }
})();

// A small player bar over the video the pointer is on.
//
// Everything here exists because a browser is not a player. Picture-in-picture may only be
// started from a trusted click, and a click on the strip belongs to another webview - so the
// control has to live in the page. Once it does, the rest of what a player offers can live
// beside it, because these are all things the page's own <video> already knows how to do.
(function () {
  var cfg = window.__mixBrowsing || {};
  if (!cfg.pipButton) return;
  var tauri = window.__TAURI_INTERNALS__;
  if (!tauri || !tauri.invoke) return;

  var LABEL = cfg.pipLabel || "Picture-in-picture";
  var BAR_ID = "__mix-player";
  var RATES = [0.5, 0.75, 1, 1.25, 1.5, 2];

  var bar = null;
  var rateButton = null;
  var current = null;
  var placeTimer = null;
  var hideTimer = null;
  var shown = false;
  var hovering = false;

  var say = function (message) {
    tauri.invoke("ui_log", { message: message }).catch(function () {});
  };

  var canPip = typeof HTMLVideoElement.prototype.requestPictureInPicture === "function";

  /// The site's own icon, as a url the media session will accept.
  ///
  /// The same lookup the tab strip uses: the declared icon when there is one, and
  /// `/favicon.ico` otherwise. Nothing is filtered by type: whether the platform can draw a
  /// given source is its decision, and skipping a site's icon because this code guessed wrong
  /// about the format would leave the window with nothing at all.
  var siteIcon = function () {
    var link = document.querySelector('link[rel~="icon"]');
    var href = link && link.href ? link.href : "";
    if (!href && location.protocol.indexOf("http") === 0) {
      href = location.origin + "/favicon.ico";
    }
    return href || "";
  };

  /// Gives the media session the site's icon, unless the page has already filled it in.
  ///
  /// The window draws whatever artwork the platform is handed, and only the page can hand it
  /// over. A page that set its own artwork knows best - a video site puts a cover there - so
  /// this only fills the empty case, where the window would otherwise show nothing at all.
  var dressMetadata = function () {
    if (!("mediaSession" in navigator) || typeof MediaMetadata !== "function") return;
    try {
      var held = navigator.mediaSession.metadata;
      var artwork = (held && held.artwork) || [];
      if (artwork.length) return;
      var icon = siteIcon();
      if (!icon) return;
      navigator.mediaSession.metadata = new MediaMetadata({
        title: (held && held.title) || document.title || "",
        artist: (held && held.artist) || location.hostname,
        album: (held && held.album) || "",
        artwork: [{ src: icon, sizes: "96x96" }],
      });
    } catch (error) {
      // A page can freeze its own media session; nothing here is worth failing over.
    }
  };

  // --- the bar ------------------------------------------------------------

  var label = function (text, title) {
    var node = document.createElement("button");
    node.type = "button";
    node.textContent = text;
    if (title) node.title = title;
    node.setAttribute("aria-label", title || text);
    return node;
  };

  var ensureBar = function () {
    if (bar && bar.isConnected) return bar;
    bar = document.createElement("div");
    bar.id = BAR_ID;
    bar.style.cssText = [
      "position:fixed",
      "z-index:2147483647",
      "display:flex",
      "align-items:center",
      "gap:2px",
      "padding:3px",
      "border-radius:8px",
      "background:rgba(16,18,20,.72)",
      "box-shadow:0 2px 8px rgba(0,0,0,.45)",
      "opacity:0",
      "transition:opacity .12s linear",
      "pointer-events:auto",
    ].join(";");

    var pip = label("\u25f1", LABEL);
    pip.addEventListener(
      "click",
      function (event) {
        event.stopPropagation();
        event.preventDefault();
        togglePip();
      },
      true,
    );
    if (!canPip) pip.style.display = "none";

    rateButton = label("1x", "Speed");
    rateButton.style.minWidth = "34px";
    rateButton.addEventListener(
      "click",
      function (event) {
        event.stopPropagation();
        event.preventDefault();
        cycleRate();
      },
      true,
    );

    var quiet = label("\u266a", "Mute");
    quiet.addEventListener(
      "click",
      function (event) {
        event.stopPropagation();
        event.preventDefault();
        if (!current) return;
        current.muted = !current.muted;
        quiet.textContent = current.muted ? "\u2715" : "\u266a";
      },
      true,
    );

    // Only offered for a plain file. A segmented stream answers one segment, which is not the
    // video, and a row that produced a file that will not play would be worse than no row.
    var save = label("\u2b07", "Save the video file");
    save.addEventListener(
      "click",
      function (event) {
        event.stopPropagation();
        event.preventDefault();
        saveVideo();
      },
      true,
    );

    [pip, rateButton, quiet, save].forEach(function (node) {
      node.style.cssText =
        "width:26px;height:26px;padding:0;border:0;border-radius:6px;background:transparent;" +
        "color:#fff;cursor:pointer;font:13px/1 system-ui,sans-serif;display:grid;place-items:center";
      bar.append(node);
    });

    // The page decides what a press on top of its video means; none of it belongs here.
    ["pointerdown", "mousedown", "mouseup", "dblclick"].forEach(function (name) {
      bar.addEventListener(
        name,
        function (event) {
          event.stopPropagation();
        },
        true,
      );
    });

    // Not `documentElement`: an initialization script runs before the parser has made one, so
    // the first placement would throw on a null parent.
    (document.body || document.documentElement || document).append(bar);
    return bar;
  };

  var togglePip = function () {
    if (!current) return;
    if (document.pictureInPictureElement) {
      document.exitPictureInPicture().catch(function () {});
      return;
    }
    dressMetadata();
    // Called from inside the click, which is what the engine requires.
    var started = current.requestPictureInPicture();
    if (started && started.catch) {
      started.catch(function (error) {
        say("pip refused: " + ((error && error.name) || "") + " " + ((error && error.message) || ""));
      });
    }
  };

  var cycleRate = function () {
    if (!current) return;
    var at = RATES.indexOf(current.playbackRate);
    var next = RATES[(at + 1) % RATES.length];
    current.playbackRate = next;
    if (rateButton) rateButton.textContent = next === 1 ? "1x" : next + "x";
  };

  /// Says something in the page, for a reader who is looking at the page.
  ///
  /// `say` writes to the log, which nobody watching a video will read. Every refusal on this
  /// bar used to be one of those - a download that did nothing at all and left no trace the
  /// reader could see.
  var notify = function (text) {
    var pill = document.createElement("div");
    pill.textContent = text;
    pill.style.cssText = [
      "position:fixed",
      "left:50%",
      "bottom:48px",
      "transform:translateX(-50%)",
      "z-index:2147483647",
      "padding:8px 16px",
      "border-radius:10px",
      "background:rgba(24,26,28,.94)",
      "color:#e8eaed",
      "border:1px solid rgba(255,255,255,.16)",
      "font:13px/1.4 system-ui,sans-serif",
      "pointer-events:none",
      "max-width:70vw",
    ].join(";");
    (document.body || document.documentElement).append(pill);
    setTimeout(function () {
      if (pill.parentNode) pill.parentNode.removeChild(pill);
    }, 3200);
  };

  var saveVideo = function () {
    var video = current;
    if (!video) return;
    var src = video.currentSrc || video.src || "";
    if (!src) {
      notify("This video has no address to save.");
      return;
    }
    // A page that assembles its own stream gives it a `blob:` address, and that address
    // answers only inside the page. There is no file behind it to fetch, so the honest thing
    // is to say that rather than start a download that would fail or save nonsense.
    if (src.indexOf("blob:") === 0 || src.indexOf("data:") === 0) {
      notify("This video is streamed in pieces, so there is no single file to save.");
      say("save refused: blob or inline source");
      return;
    }
    var name = (src.split("/").pop() || "video").split("?")[0];
    notify("Saving " + name + "\u2026");
    tauri.invoke("video_download", { url: src, fileName: name }).then(
      function () {
        notify("Saved to the downloads folder.");
      },
      function (error) {
        var detail = (error && (error.message || error.code)) || String(error);
        notify("Could not save: " + detail);
        say("save video failed: " + detail);
      },
    );
  };

  /// The video a reader would call "the video": visible, ready, and the biggest one.
  ///
  /// Not "the one that is playing". A reader who paused and then wants it out of the way is
  /// asking for exactly this. What is excluded is a video that has never loaded anything: a
  /// page leaves those in the document as placeholders, and they are the ones that would make
  /// the bar point at nothing.
  var pick = function () {
    var best = null;
    var bestArea = 0;
    var videos = document.querySelectorAll("video");
    for (var i = 0; i < videos.length; i += 1) {
      var video = videos[i];
      // `readyState` has to be 1 rather than 2: a paused video drops back to HAVE_METADATA,
      // so a threshold of 2 excluded exactly the case this is for.
      if (video.readyState < 1) continue;
      if (video.ended) continue;
      if (video === document.pictureInPictureElement) continue;
      var box = video.getBoundingClientRect();
      if (box.width < 160 || box.height < 90) continue;
      if (box.right < 0 || box.bottom < 0) continue;
      if (box.left > window.innerWidth || box.top > window.innerHeight) continue;
      var area = box.width * box.height;
      if (area > bestArea) {
        bestArea = area;
        best = video;
      }
    }
    return best;
  };

  var show = function () {
    if (!bar) return;
    clearTimeout(hideTimer);
    hideTimer = null;
    shown = true;
    bar.style.opacity = "1";
  };

  var fade = function () {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(function () {
      hideTimer = null;
      if (bar && !hovering) {
        shown = false;
        bar.style.opacity = "0";
      }
    }, 900);
  };

  var place = function () {
    placeTimer = null;
    var video = pick();
    if (!video) {
      current = null;
      hovering = false;
      if (bar) bar.style.opacity = "0";
      return;
    }
    var changed = video !== current;
    current = video;
    ensureBar();
    if (rateButton) {
      rateButton.textContent = video.playbackRate === 1 ? "1x" : video.playbackRate + "x";
    }
    var box = video.getBoundingClientRect();
    var width = bar.offsetWidth || 128;
    // Inside the video's top-right corner: a page that clips its own overflow would cut a bar
    // hanging off the edge, and a video that fills the window has no outside.
    var inset = 10;
    var left = Math.min(Math.max(box.left + box.width - width - inset, 4), window.innerWidth - width - 4);
    var top = Math.min(Math.max(box.top + inset, 4), window.innerHeight - 34);
    bar.style.left = Math.round(left) + "px";
    bar.style.top = Math.round(top) + "px";

    // Drawn once when a video starts, and after that only while the pointer is near it.
    if (changed) show();
    else if (hovering) show();
  };

  /// Whether the pointer is in the region the bar occupies.
  var near = function (x, y) {
    if (!current) return false;
    var box = current.getBoundingClientRect();
    var width = (bar && bar.offsetWidth) || 128;
    return (
      x >= box.right - width - 40 &&
      x <= box.right + 10 &&
      y >= box.top - 10 &&
      y <= box.top + 60
    );
  };

  var soon = function () {
    if (placeTimer) return;
    placeTimer = requestAnimationFrame(place);
  };

  window.addEventListener("resize", soon, { passive: true });
  document.addEventListener("scroll", soon, { passive: true, capture: true });
  document.addEventListener(
    "pointermove",
    function (event) {
      var inside = near(event.clientX, event.clientY);
      if (inside === hovering) return;
      hovering = inside;
      if (inside) show();
      else if (shown) fade();
    },
    { passive: true, capture: true },
  );
  document.addEventListener("mouseleave", function () {
    hovering = false;
    if (shown) fade();
  });
  ["play", "pause", "ended", "loadedmetadata", "emptied", "ratechange"].forEach(function (name) {
    document.addEventListener(
      name,
      function (event) {
        if (!event.target || event.target.tagName !== "VIDEO") return;
        if (name !== "ended" && name !== "emptied") current = null;
        if (name === "play" || name === "pause" || name === "loadedmetadata") dressMetadata();
        soon();
      },
      true,
    );
  });
  if (window.MutationObserver) {
    new MutationObserver(soon).observe(document.documentElement || document, {
      childList: true,
      subtree: true,
    });
  }
  setInterval(soon, 250);
  document.addEventListener("enterpictureinpicture", function () {
    hovering = false;
    shown = false;
    if (bar) bar.style.opacity = "0";
  });
  soon();
})();


// Keyboard control for the video the pointer is on.
//
// The browser has no player of its own, so a video only answers the keys a site chose to
// implement - and on most sites that is nothing beyond the space bar. These are the keys a
// desktop player has always had.
//
// Scoped to the video under the pointer, or the one holding the keyboard focus. A page has
// its own use for every one of these keys - a search box, a scroll, a site's own shortcut -
// and answering them everywhere would be taking the page's keyboard away. A reader with the
// pointer resting on a playing video is asking for that video.
//
// Every handled key is stopped in the capture phase, so a site's own handler never sees a
// press that was answered here: space plays and pauses in this browser, and a page that also
// listens for space would otherwise toggle it a second time and undo it.
(function () {
  var cfg = window.__mixBrowsing || {};
  if (!cfg.videoKeys) return;

  var STEP = 10;
  var VOLUME_STEP = 0.1;
  var pointer = { x: -1, y: -1 };

  document.addEventListener(
    "pointermove",
    function (event) {
      pointer.x = event.clientX;
      pointer.y = event.clientY;
    },
    { passive: true, capture: true },
  );
  document.addEventListener(
    "mouseleave",
    function () {
      pointer.x = -1;
      pointer.y = -1;
    },
    { passive: true, capture: true },
  );

  var inside = function (video) {
    if (!video) return false;
    var box = video.getBoundingClientRect();
    if (box.width < 2 || box.height < 2) return false;
    return (
      pointer.x >= box.left &&
      pointer.x <= box.right &&
      pointer.y >= box.top &&
      pointer.y <= box.bottom
    );
  };

  /// The video these keys belong to: the one under the pointer, else the focused one.
  var target = function () {
    var focused = document.activeElement;
    if (focused && focused.tagName === "VIDEO") return focused;
    var best = null;
    var bestArea = 0;
    var videos = document.querySelectorAll("video");
    for (var i = 0; i < videos.length; i += 1) {
      var video = videos[i];
      if (video.disablePictureInPicture && video.readyState === 0) continue;
      if (!inside(video)) continue;
      var box = video.getBoundingClientRect();
      var area = box.width * box.height;
      if (area > bestArea) {
        bestArea = area;
        best = video;
      }
    }
    return best;
  };

  /// Whether the press belongs to something else: a field, a modifier, or a key repeat.
  var belongsElsewhere = function (event) {
    if (event.ctrlKey || event.metaKey || event.altKey) return true;
    // A held key would run the step dozens of times a second, which is not a step.
    if (event.repeat) return true;
    var node = event.target;
    if (!node || !node.tagName) return false;
    var tag = node.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
    if (node.isContentEditable) return true;
    return false;
  };

  var seek = function (video, delta) {
    if (!isFinite(video.duration)) return;
    var wanted = video.currentTime + delta;
    video.currentTime = Math.min(Math.max(wanted, 0), Math.max(video.duration - 0.1, 0));
  };

  var volume = function (video, delta) {
    video.muted = false;
    video.volume = Math.min(Math.max(video.volume + delta, 0), 1);
  };

  document.addEventListener(
    "keydown",
    function (event) {
      if (belongsElsewhere(event)) return;
      var video = target();
      if (!video) return;

      var handled = true;
      switch (event.key) {
        case " ":
        case "Spacebar":
          if (video.paused) video.play().catch(function () {});
          else video.pause();
          break;
        case "ArrowLeft":
          seek(video, -STEP);
          break;
        case "ArrowRight":
          seek(video, STEP);
          break;
        case "ArrowUp":
          volume(video, VOLUME_STEP);
          break;
        case "ArrowDown":
          volume(video, -VOLUME_STEP);
          break;
        case "m":
        case "M":
          video.muted = !video.muted;
          break;
        case "f":
        case "F":
          if (document.fullscreenElement) document.exitFullscreen().catch(function () {});
          else if (video.requestFullscreen) video.requestFullscreen().catch(function () {});
          break;
        default:
          handled = false;
      }
      if (!handled) return;
      event.preventDefault();
      event.stopPropagation();
    },
    true,
  );
})();

// WebView2's own error page.
(function () {
  var check = function () {
    try {
      var text = document.body ? document.body.innerText || "" : "";
      var match = /ERR_[A-Z_]+/.exec(text);
      if (!match) return;
      window.__TAURI_INTERNALS__.invoke("page_failed", { code: match[0] }).catch(function () {});
    } catch (error) {
      /* ignore */
    }
  };
  document.addEventListener("DOMContentLoaded", function () {
    setTimeout(check, 600);
  });
})();

// Scroll position memory.
(function () {
  var timer = null;
  window.addEventListener("scroll", function () {
    if (timer) return;
    timer = setTimeout(function () {
      timer = null;
      window.__TAURI_INTERNALS__.invoke("tab_scroll", { y: window.scrollY }).catch(function () {});
    }, 400);
  }, { passive: true });
})();

// Keys forwarded while the address bar owns the keyboard.
(function () {
  var forward = function (kind, value) {
    var payload = kind === "type" ? { text: value } : { key: value };
    var command = kind === "type" ? "url_type" : "url_key";
    window.__TAURI_INTERNALS__.invoke(command, payload).catch(function () {});
  };

  var sink = null;
  var ensureSink = function () {
    if (sink) return sink;
    sink = document.createElement("textarea");
    sink.setAttribute("autocomplete", "off");
    sink.setAttribute("autocorrect", "off");
    sink.setAttribute("spellcheck", "false");
    sink.setAttribute("tabindex", "-1");
    sink.style.cssText =
      "position:fixed;left:-1000px;top:0;width:1px;height:1px;opacity:0;pointer-events:none;";
    (document.body || document.documentElement).appendChild(sink);
    var flush = function () {
      var text = sink.value;
      if (!text) return;
      sink.value = "";
      if (window.__mixUrlMode) forward("type", text);
    };
    sink.addEventListener("compositionend", flush);
    sink.addEventListener("input", function (event) {
      if (!event.isComposing) flush();
    });
    return sink;
  };

  window.addEventListener("mix-url-mode", function () {
    if (window.__mixUrlMode) {
      try {
        ensureSink().focus();
      } catch (error) {}
    } else if (sink) {
      sink.blur();
    }
  }, true);

  var EDIT_KEYS = {
    ArrowLeft: "left",
    ArrowRight: "right",
    ArrowUp: "up",
    ArrowDown: "down",
    Home: "home",
    End: "end",
    Delete: "delete"
  };

  window.addEventListener("keydown", function (event) {
    if (!window.__mixUrlMode) return;
    if (event.isComposing || event.keyCode === 229) return;
    var stop = function () {
      event.preventDefault();
      event.stopPropagation();
    };
    if ((event.ctrlKey || event.metaKey) && !event.altKey) {
      var combo = event.key.toLowerCase();
      if (combo === "a") {
        forward("key", "select-all");
        stop();
        return;
      }
      if (combo === "v" || combo === "c" || combo === "x") {
        forward("key", combo === "v" ? "paste" : combo);
        stop();
        return;
      }
      if (combo === "backspace") {
        forward("key", "delete-word");
        stop();
      }
      return;
    }
    if (event.altKey) return;
    if (event.key.length === 1) {
      forward("type", event.key);
      stop();
      return;
    }
    var edit = EDIT_KEYS[event.key];
    if (edit) {
      forward("key", edit);
      stop();
      return;
    }
    if (event.key === "Backspace") {
      forward("type", "\u0008");
      stop();
      return;
    }
    if (event.key === "Enter") {
      forward("key", "enter");
      stop();
      return;
    }
    if (event.key === "Escape") {
      forward("key", "escape");
      stop();
    }
  }, true);
})();

(function () {
  if (window.__mixKeys) return;
  window.__mixKeys = true;

  var send = function (action) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: action }).catch(function () {});
  };

  var comboOf = function (event) {
    var parts = [];
    if (event.ctrlKey || event.metaKey) parts.push("ctrl");
    if (event.altKey) parts.push("alt");
    if (event.shiftKey) parts.push("shift");
    var key = event.key.length === 1 ? event.key.toLowerCase() : event.key.toLowerCase();
    if (key === " ") key = "space";
    parts.push(key);
    return parts.join("+");
  };

  var custom = function () {
    var map = window.__mixShortcuts || null;
    if (!map) return {};
    var byCombo = {};
    for (var action in map) byCombo[map[action]] = action;
    return byCombo;
  };

  var actionFor = function (event) {
    var overrides = custom();
    if (overrides) {
      var hit = overrides[comboOf(event)];
      if (hit) return hit;
    }
    if (event.key === "Escape") return "close_overlay";
    if (event.key === "F5") return "reload";
    if (event.key === "F11") return "fullscreen";
    if (event.key === "F12") return "devtools";
    var ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) return null;
    var key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    switch (key) {
      case "t": return event.shiftKey ? "reopen_tab" : "new_tab";
      case "w": return "close_tab";
      case "l": return "focus_url";
      case "r": return "reload";
      case "Tab": return event.shiftKey ? "prev_tab" : "next_tab";
      case "=":
      case "+": return "zoom_in";
      case "-": return "zoom_out";
      case "0": return "zoom_reset";
      case "p": return event.shiftKey ? "palette" : "print";
      case "a": return event.shiftKey ? "tab_search" : null;
      case "g": return event.shiftKey ? "group_tab" : null;
      case "f": return "find";
      case "j": return "downloads";
      case "d": return "bookmark";
      case "n": return event.shiftKey ? "new_incognito" : "new_window";
      default: return null;
    }
  };

  var report = function (payload) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("context_menu", { request: payload }).catch(function () {});
  };

  window.addEventListener("contextmenu", function (event) {
    if (window.__mixGesturePending) {
      window.__mixGesturePending = false;
      event.preventDefault();
      return;
    }
    event.preventDefault();
    var link = event.target.closest ? event.target.closest("a") : null;
    var image = event.target.closest ? event.target.closest("img") : null;
    var selection = window.getSelection ? String(window.getSelection()) : "";
    menuShownAt = Date.now();
    report({
      dismiss: false,
      x: Math.round(event.clientX),
      y: Math.round(event.clientY),
      link: link ? link.href : null,
      image: image ? (image.currentSrc || image.src) : null,
      selection: selection ? selection.slice(0, 200) : null,
      editable: !!(event.target.isContentEditable || /^(input|textarea)$/i.test(event.target.tagName || ""))
    });
  }, true);

  var menuShownAt = 0;
  window.addEventListener("mousedown", function () {
    if (Date.now() - menuShownAt > 8000) return;
    menuShownAt = 0;
    report({ dismiss: true });
  }, true);

  var pollPermission = function (invoke, id) {
    return new Promise(function (resolve) {
      var tries = 0;
      var tick = function () {
        invoke("permission_poll", { id: id }).then(function (value) {
          if (typeof value === "boolean") {
            resolve(value);
            return;
          }
          tries += 1;
          if (tries > 300) {
            resolve(false);
            return;
          }
          setTimeout(tick, 200);
        }).catch(function () { resolve(false); });
      };
      tick();
    });
  };

  var askPermission = function (kind) {
    var tauri = window.__TAURI_INTERNALS__;
    if (!tauri || !tauri.invoke) return Promise.resolve(false);
    var invoke = function (command, payload) { return tauri.invoke(command, payload); };
    // The origin is not sent: the command reads it from the webview that called it, because
    // a page that can name its own origin can ask for permission as somebody else.
    return invoke("permission_request", {
      kind: kind
    }).then(function (answer) {
      if (!answer) return false;
      if (typeof answer.allow === "boolean") return answer.allow;
      return pollPermission(invoke, answer.id);
    }).catch(function () { return false; });
  };

  if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
    var originalMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = function (constraints) {
      var kind = constraints && constraints.video ? "camera" : "microphone";
      return askPermission(kind).then(function (allow) {
        if (!allow) throw new DOMException("Permission denied", "NotAllowedError");
        return originalMedia(constraints);
      });
    };
  }

  if (navigator.geolocation) {
    ["getCurrentPosition", "watchPosition"].forEach(function (name) {
      var original = navigator.geolocation[name].bind(navigator.geolocation);
      navigator.geolocation[name] = function (success, failure, options) {
        askPermission("geolocation").then(function (allow) {
          if (allow) return original(success, failure, options);
          if (failure) failure({ code: 1, message: "Permission denied" });
        });
      };
    });
  }

  if (window.Notification && Notification.requestPermission) {
    var originalNotify = Notification.requestPermission.bind(Notification);
    Notification.requestPermission = function (callback) {
      return askPermission("notifications").then(function (allow) {
        if (allow) return originalNotify(callback);
        var denied = "denied";
        if (callback) callback(denied);
        return denied;
      });
    };
  }

  // Window docking used to be collected here as well, as a fallback for a machine where
  // the global shortcut could not be registered. It is not coming back: this script runs
  // inside whatever page is open, so any site could move and resize the user's window by
  // pressing the keys itself. The strip has its own handler over a page we serve, and the
  // global shortcut covers the rest.
  window.addEventListener("keydown", function (event) {
    var action = actionFor(event);
    if (!action) return;
    if (action !== 'close_overlay') {
      event.preventDefault();
      event.stopPropagation();
    }
    send(action);
  }, true);

  window.addEventListener("pointerdown", function () {
    if (!window.__mixOverlay) return;
    window.__mixOverlay = false;
    send("close_overlay");
  }, true);

  // Ctrl and the wheel zoom natively otherwise, which leaves the app with a stale zoom:
  // the panel shows 100% and the per site level is never recorded. The scroll is
  // collected until it is worth one step, so a pinch does not run away.
  var wheelAccum = 0;
  window.addEventListener("wheel", function (event) {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    wheelAccum += event.deltaY;
    if (Math.abs(wheelAccum) < 50) return;
    send(wheelAccum < 0 ? "zoom_in" : "zoom_out");
    wheelAccum = 0;
  }, { passive: false, capture: true });
})();

(function () {
  if (window.__mixGestureEngine) return;
  window.__mixGestureEngine = true;

  var cfg = window.__mixGestureConfig || {};
  var MAP = cfg.map || {};
  var THRESHOLD = cfg.threshold || 34;
  var COLOR = cfg.color || '#4c8dff';
  var WIDTH = cfg.width || 3;
  var TRAIL = cfg.trail !== false;
  var BLOCKED = String(cfg.blocked || '')
    .split(/[,\s]+/)
    .filter(Boolean);
  function send(action) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) return;
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: action }).catch(function () {});
  }

  var host = location.hostname.replace(/^www\./, '').toLowerCase();
  var disabled =
    window.__mixGestures === false ||
    BLOCKED.some(function (entry) {
      var name = entry.replace(/^www\./, '').toLowerCase();
      return host === name || host.endsWith('.' + name);
    });
  var drawing = false;
  var dirs = [];
  var last = null;
  var fromX = 0;
  var fromY = 0;
  var canvas = null;
  var context = null;

  function trail() {
    if (canvas) return;
    canvas = document.createElement("canvas");
    canvas.style.cssText = "position:fixed;inset:0;width:100%;height:100%;z-index:2147483647;pointer-events:none;";
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    context = canvas.getContext("2d");
    context.strokeStyle = COLOR;
    context.lineWidth = WIDTH;
    context.lineCap = "round";
    document.documentElement.appendChild(canvas);
  }

  function clearTrail() {
    if (canvas && canvas.parentNode) canvas.parentNode.removeChild(canvas);
    canvas = null;
    context = null;
  }

  window.addEventListener("mousedown", function (event) {
    if (disabled) return;
    if (event.button !== 2) return;
    drawing = true;
    dirs = [];
    last = null;
    fromX = event.clientX;
    fromY = event.clientY;
    if (TRAIL) trail();
    if (context) { context.beginPath(); context.moveTo(fromX, fromY); }
  }, true);

  window.addEventListener("mousemove", function (event) {
    if (!drawing) return;
    if (context) { context.lineTo(event.clientX, event.clientY); context.stroke(); }
    var dx = event.clientX - fromX;
    var dy = event.clientY - fromY;
    if (Math.abs(dx) < THRESHOLD && Math.abs(dy) < THRESHOLD) return;
    var dir = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? "R" : "L") : dy > 0 ? "D" : "U";
    if (dir !== last) { dirs.push(dir); last = dir; }
    fromX = event.clientX;
    fromY = event.clientY;
  }, true);

  window.addEventListener("mouseup", function (event) {
    if (event.button !== 2 || !drawing) return;
    drawing = false;
    clearTrail();
    var pattern = dirs.join("");
    if (!pattern) return;
    event.preventDefault();
    event.stopPropagation();
    window.__mixGesturePending = true;
    var action = MAP[pattern];
    if (!action || action === "none") return;
    if (action === "scroll_top") {
      window.scrollTo({ top: 0, behavior: "smooth" });
      return;
    }
    if (action === "scroll_bottom") {
      window.scrollTo({ top: document.body.scrollHeight, behavior: "smooth" });
      return;
    }
    send(action);
  }, true);

  window.addEventListener("blur", function () { drawing = false; clearTrail(); });
})();


(function () {
  if (window.__mixSuperDragEngine) return;
  window.__mixSuperDragEngine = true;
  if (window.__mixSuperDrag === false) return;

  var cfg = window.__mixDragConfig || {};
  var STEP = cfg.threshold || 28;
  var HINT = cfg.hint !== false;
  var MAP = cfg.map || { L: 'open', R: 'search', U: 'copy', D: 'save' };
  var drag = null;
  var badge = null;
  var lastAction = 0;

  function toast(text) {
    var pill = document.createElement('div');
    pill.textContent = text;
    pill.style.cssText =
      'position:fixed;left:50%;bottom:32px;transform:translateX(-50%);z-index:2147483647;' +
      'padding:6px 14px;border-radius:14px;background:#202124;color:#e8eaed;' +
      'border:1px solid #4c8dff;font:12px sans-serif;pointer-events:none;';
    document.documentElement.appendChild(pill);
    setTimeout(function () {
      if (pill.parentNode) pill.parentNode.removeChild(pill);
    }, 2200);
  }

  function invoke(command, args) {
    if (!window.__TAURI_INTERNALS__ || !window.__TAURI_INTERNALS__.invoke) {
      toast('super drag: no bridge');
      return;
    }
    var pending = window.__TAURI_INTERNALS__.invoke(command, args);
    if (pending && pending.catch) {
      pending.catch(function (error) {
        toast('super drag failed: ' + command);
        if (window.__TAURI_INTERNALS__.invoke) {
          window.__TAURI_INTERNALS__.invoke('ui_log', {
            message: 'super drag failed: ' + command + ' -> ' + String(error),
          }).catch(function () {});
        }
      });
    }
  }

  function linkOf(node) {
    if (!node || !node.closest) return null;
    var anchor = node.closest('a');
    if (anchor) {
      var href = anchor.href || anchor.getAttribute('data-href') || anchor.getAttribute('href') || '';
      return { element: anchor, href: href };
    }
    var holder = node.closest('[data-href],[data-url],[data-link]');
    if (holder) {
      return {
        element: holder,
        href:
          holder.getAttribute('data-href') ||
          holder.getAttribute('data-url') ||
          holder.getAttribute('data-link') ||
          '',
      };
    }
    return null;
  }

  function imageOf(node) {
    if (!node || !node.closest) return null;
    var image = node.closest('img');
    return image && image.src ? image : null;
  }

  function showBadge(mark) {
    if (!HINT) return;
    if (!badge) {
      badge = document.createElement('div');
      badge.style.cssText =
        'position:fixed;z-index:2147483647;pointer-events:none;padding:2px 8px;border-radius:10px;' +
        'background:#4c8dff;color:#fff;font:12px sans-serif;opacity:.92;';
      document.documentElement.appendChild(badge);
    }
    badge.textContent = mark;
  }

  function hideBadge() {
    if (badge && badge.parentNode) badge.parentNode.removeChild(badge);
    badge = null;
  }

  function sourceFromEvent(event) {
    var path = [];
    try {
      path = event.composedPath ? event.composedPath() : [];
    } catch (error) {}
    for (var index = 0; index < path.length; index += 1) {
      var node = path[index];
      if (!node || node.nodeType !== 1) continue;
      var found = pickSource(node, true);
      if (found) return found;
    }
    return sourceAtPoint(event.clientX, event.clientY, event.target);
  }

  function sourceAtPoint(x, y, fallbackTarget) {
    var stack = [];
    try {
      stack = document.elementsFromPoint(x, y) || [];
    } catch (error) {}
    for (var index = 0; index < stack.length; index += 1) {
      var found = pickSource(stack[index], true);
      if (found) return found;
    }
    return pickSource(fallbackTarget);
  }

  function pickSource(target, quiet) {
    var link = linkOf(target);
    if (link) {
      var label = String(link.element.textContent || '').trim().slice(0, 300);
      if (link.href) {
        return { url: link.href, text: '', linkText: label };
      }
      if (label) {
        return { url: '', text: label, linkText: label };
      }
    }
    var image = imageOf(target);
    if (image) {
      return { url: image.src, text: '', linkText: '' };
    }
    var selected = String(window.getSelection ? window.getSelection() : '').trim();
    if (selected) {
      return { url: '', text: selected, linkText: '' };
    }

    if (quiet) return null;
    var node = target;
    for (var depth = 0; depth < 6 && node && node.nodeType === 1; depth += 1) {
      var style = null;
      try {
        style = window.getComputedStyle(node);
      } catch (error) {}
      var looksClickable =
        (style && style.cursor === 'pointer') ||
        node.getAttribute('role') === 'link' ||
        node.hasAttribute('tabindex') ||
        node.hasAttribute('onclick');
      if (looksClickable) {
        var label = String(node.textContent || '').trim().slice(0, 120);
        if (label) {
          return { url: '', text: label, linkText: label };
        }
      }
      node = node.parentElement;
    }
    return null;
  }

  window.addEventListener('pointerdown', function (event) {
    if (event.button !== 0) return;
    var source = sourceFromEvent(event);
    if (!source) return;
    drag = {
      url: source.url,
      text: source.text,
      linkText: source.linkText,
      x: event.clientX,
      y: event.clientY,
      moved: false,
      pointer: event.pointerId,
      element: event.target,
      armedAt: Date.now(),
    };
  }, true);

  window.addEventListener('dragstart', function (event) {
    if (drag) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, true);

  window.addEventListener('pointermove', function (event) {
    if (!drag) return;
    var dx = event.clientX - drag.x;
    var dy = event.clientY - drag.y;
    if (!drag.moved) {
      if (Math.abs(dx) < STEP && Math.abs(dy) < STEP) return;
      drag.moved = true;
      try {
        if (drag.element && drag.element.setPointerCapture) {
          drag.element.setPointerCapture(drag.pointer);
        }
      } catch (error) {}
      document.documentElement.style.userSelect = 'none';
      showBadge(drag.text ? '\uD83D\udd0d' : '\u2197');
    }
    if (badge) {
      badge.style.left = event.clientX + 14 + 'px';
      badge.style.top = event.clientY + 14 + 'px';
    }
  }, true);

  function finish(event) {
    if (!drag) return;
    var item = drag;
    drag = null;
    hideBadge();
    document.documentElement.style.userSelect = '';
    try {
      if (item.element && item.element.releasePointerCapture) {
        item.element.releasePointerCapture(item.pointer);
      }
    } catch (error) {}
    if (!item.moved) return;
    var now = Date.now();
    if (now - lastAction < 400) return;
    lastAction = now;

    event.preventDefault();
    event.stopPropagation();

    var dx = event.clientX - item.x;
    var dy = event.clientY - item.y;
    var dir = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'R' : 'L') : dy > 0 ? 'D' : 'U';
    var action = MAP[dir] || 'none';
    if (action === 'none') return;

    var text = item.text;
    var url = item.url;
    var linkText = item.linkText || '';

    if (action === 'search') {
      invoke('drag_action', { kind: 'search', text: text || linkText || url || '' });
      return;
    }
    if (action === 'copy') {
      var value = text || url || linkText;
      if (value) invoke('drag_action', { kind: 'copy', text: value });
      return;
    }
    if (action === 'copy_link_text') {
      if (linkText) invoke('drag_action', { kind: 'copy', text: linkText });
      return;
    }
    if (action === 'copy_link_both') {
      if (url) invoke('drag_action', { kind: 'copy', text: (linkText ? linkText + '\n' : '') + url });
      return;
    }
    if (action === 'copy_sel_and_url') {
      var picked = text || linkText;
      if (picked) {
        invoke('drag_action', { kind: 'copy', text: picked + '\n' + location.href });
      } else {
        invoke('drag_action', { kind: 'copy', text: location.href });
      }
      return;
    }
    if (action === 'find_next' || action === 'find_prev') {
      var needle = text || linkText;
      if (!needle) return;
      try {
        window.find(needle, false, action === 'find_prev', true, false, false, false);
      } catch (error) {}
      return;
    }
    if (action === 'search_in_site') {
      var query = text || linkText;
      if (!query) return;
      var host = location.hostname.replace(/^www\./, '');
      invoke('drag_action', { kind: 'search', text: query + ' site:' + host });
      return;
    }
    if (action === 'save') {
      if (url) invoke('drag_action', { kind: 'save', url: url });
      return;
    }
    if (action === 'open_background') {
      if (url) invoke('drag_action', { kind: 'open', url: url, background: true });
      else if (text) invoke('drag_action', { kind: 'search', text: text });
      return;
    }
    if (url) invoke('drag_action', { kind: 'open', url: url });
    else if (text) invoke('drag_action', { kind: 'search', text: text });
  }

  window.addEventListener('pointerup', finish, true);

  window.addEventListener('mouseup', function (event) {
    if (!drag) return;
    finish(event);
  }, true);
  window.addEventListener('pointercancel', function () {
    drag = null;
    hideBadge();
    document.documentElement.style.userSelect = '';
  }, true);

  window.addEventListener('blur', function () {
    drag = null;
    hideBadge();
    document.documentElement.style.userSelect = '';
  });
})();

// Reading mode: the article, without the page around it.
//
// The extraction is a heuristic, and deliberately a small one: the block with the most
// text that is not navigation. A real reader scores every candidate against a model of
// what an article looks like; that is a library, and this is the part of one that matters
// for the pages a person actually reads - a news story, a blog post, a documentation
// page.
(function () {
  if (window.__mixReadable) return;
  window.__mixReadable = true;

  var NOISE = 'script,style,noscript,nav,header,footer,aside,form,iframe,button,svg';

  var score = function (element) {
    var text = (element.innerText || '').trim();
    if (text.length < 200) return 0;
    var links = element.querySelectorAll('a');
    var linkText = 0;
    for (var i = 0; i < links.length; i++) linkText += (links[i].innerText || '').length;
    // A block that is mostly links is a menu or a footer, not a story.
    var density = linkText / Math.max(1, text.length);
    if (density > 0.5) return 0;
    var paragraphs = element.querySelectorAll('p').length;
    return text.length * (1 - density) + paragraphs * 30;
  };

  var extract = function () {
    var candidates = document.querySelectorAll('article, main, [role=main], section, div');
    var best = null;
    var bestScore = 0;
    for (var i = 0; i < candidates.length; i++) {
      var value = score(candidates[i]);
      if (value > bestScore) {
        bestScore = value;
        best = candidates[i];
      }
    }
    if (!best) best = document.body;
    var clone = best.cloneNode(true);
    var junk = clone.querySelectorAll(NOISE);
    for (var j = junk.length - 1; j >= 0; j--) junk[j].remove();

    // Every image is resolved to an absolute address, because the document it lands in
    // has no base of its own.
    var images = clone.querySelectorAll('img');
    for (var k = 0; k < images.length; k++) {
      var src = images[k].currentSrc || images[k].src || '';
      if (src) images[k].setAttribute('src', src);
      else images[k].remove();
    }

    var title = document.querySelector('h1') || document.querySelector('title');
    return {
      title: (title && (title.innerText || title.textContent) || document.title || '').trim(),
      byline: (document.querySelector('meta[name=author]') || {}).content || '',
      html: clone.innerHTML.slice(0, 400000),
      url: location.href
    };
  };

  window.__mixExtractArticle = extract;
})();


// Browsing habits: the things a reader expects from a desktop browser and notices when they
// are missing. Each is off unless the settings say otherwise, because all three change what
// a click or a selection does on every page.
(function () {
  var cfg = window.__mixBrowsing || {};
  var tauri = window.__TAURI_INTERNALS__;
  if (!tauri || !tauri.invoke) return;

  var send = function (command, payload) {
    return tauri.invoke(command, payload);
  };

  // --- the bar revealed at the left edge ------------------------------------
  // The pointer touching the window's left edge brings the bookmarks bar out. Only the
  // crossing is reported, not every move: the wait and the hiding are the strip's business,
  // and a message per mousemove would be one ipc call per pixel.
  if (cfg.revealEdge > 0) {
    // `wasInside` starts false because the pointer starts outside the edge. Only a change of
    // that answer is worth a message.
    var wasInside = false;
    document.addEventListener("mousemove", function (event) {
      var inside = event.clientX <= Number(cfg.revealEdge);
      if (inside === wasInside) return;
      wasInside = inside;
      send("bar_reveal", {
        inside: inside,
        delay: Number(cfg.revealDelay) || 0
      }).catch(function () {});
    }, { passive: true });
    // Leaving the window entirely is also leaving the edge.
    window.addEventListener("mouseout", function (event) {
      if (event.relatedTarget) return;
      if (!wasInside) return;
      wasInside = false;
      send("bar_reveal", { inside: false, delay: 0 }).catch(function () {});
    });
  }
  // --- where a link opens --------------------------------------------------
  // A page decides where a link goes: a new tab, a named window, the same tab. Two settings
  // override that decision, and they are the pair a reader turns on when a site insists on
  // opening everything in a tab they did not want. Only the two "new tab" answers are
  // intercepted; a link that would replace the page it is on already does what the current-tab
  // answer would ask for.
  if (cfg.forceCurrent || cfg.forceBackground) {
    var hijack = function (event) {
      if (event.defaultPrevented) return;
      if (event.button !== undefined && event.button !== 0) return;
      // A modified click is the reader asking for something specific.
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      var anchor = event.target && event.target.closest ? event.target.closest("a[href]") : null;
      if (!anchor) return;
      var target = String(anchor.getAttribute("target") || "").toLowerCase();
      var href = anchor.href;
      if (!href || !/^https?:/i.test(href)) return;
      var opensElsewhere = target && target !== "_self" && target !== "_top" && target !== "_parent";
      if (!opensElsewhere) return;
      event.preventDefault();
      event.stopPropagation();
      if (cfg.forceCurrent) {
        location.href = href;
        return;
      }
      // `drag_action` is what a page is allowed to ask for: `tab_new` is reachable only
      // from the app's own pages, so a site could not call it even from here.
      send("drag_action", { kind: "open", url: href, background: true }).catch(function () {});
    };
    document.addEventListener("click", hijack, true);
    document.addEventListener("auxclick", hijack, true);
  }
  // --- copy on selection ---------------------------------------------------
  // A selection that has settled is copied without a keystroke. Debounced, because a drag
  // produces a selection in progress and copying each intermediate state would fill the
  // clipboard with prefixes of the sentence the reader meant to take.
  if (cfg.copyOnSelect) {
    var copyTimer = null;
    var lastCopied = "";
    document.addEventListener("selectionchange", function () {
      clearTimeout(copyTimer);
      copyTimer = setTimeout(function () {
        var text = String(window.getSelection ? window.getSelection() : "").trim();
        if (!text || text === lastCopied || text.length > 20000) return;
        // Never while the reader is typing: a selection inside an input is part of editing,
        // and replacing the clipboard there would fight the field's own copy and paste.
        var active = document.activeElement;
        if (active && /^(INPUT|TEXTAREA)$/.test(active.tagName)) return;
        lastCopied = text;
        send("clipboard_write", { text: text }).catch(function () {});
      }, 220);
    });
  }

  // --- autoscroll ----------------------------------------------------------
  // Hold the middle button and move: the page scrolls with the pointer, faster the further
  // it is from where the press happened. Released by the button coming up anywhere.
  if (cfg.autoscroll) {
    var state = null;
    var frame = null;

    var stop = function () {
      if (frame) cancelAnimationFrame(frame);
      frame = null;
      state = null;
    };

    var tick = function () {
      if (!state) return;
      // Distance from the anchor, squared off so small movements are gentle and large ones
      // move quickly - the shape every autoscroll has.
      var dx = state.x - state.anchorX;
      var dy = state.y - state.anchorY;
      var step = function (delta) {
        if (Math.abs(delta) < 2) return 0;
        var magnitude = Math.min(60, (Math.abs(delta) - 2) * 0.35);
        return delta < 0 ? -magnitude : magnitude;
      };
      window.scrollBy(step(dx), step(dy));
      frame = requestAnimationFrame(tick);
    };

    document.addEventListener("mousedown", function (event) {
      if (event.button !== 1) return;
      var target = event.target;
      // A field, a link and anything editable keep their own middle-click meaning.
      if (target && target.closest && target.closest("input, textarea, [contenteditable], a")) return;
      event.preventDefault();
      state = { anchorX: event.clientX, anchorY: event.clientY, x: event.clientX, y: event.clientY };
      frame = requestAnimationFrame(tick);
    }, true);

    document.addEventListener("mousemove", function (event) {
      if (!state) return;
      state.x = event.clientX;
      state.y = event.clientY;
    }, true);

    ["mouseup", "blur"].forEach(function (name) {
      window.addEventListener(name, stop, true);
    });
    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape") stop();
    }, true);
  }
})();
