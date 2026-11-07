  // Clicking anywhere in the page closes an open overlay (menu or context menu).
  let __mixDismiss = false;
  window.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || __mixDismiss) return;
    __mixDismiss = true;
    setTimeout(() => { __mixDismiss = false; }, 100);
    window.__TAURI_INTERNALS__.invoke("shortcut", { action: "dismiss" }).catch(() => {});
  }, true);

(function () {
  // Report the page favicon once the document is parsed.
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

// In-place page translation: the page owns the DOM, Rust owns the network.
(function () {
  var nodes = [];
  // Nodes already handled, so later passes only pick up new text.
  var done = new WeakSet();
  var observer = null;
  var running = false;
  var timer = null;

  // Same origin frames share the document tree with us; cross origin ones throw.
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

  // New content (infinite scroll, SPA updates) is picked up after it settles.
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

// Video players ask the document for fullscreen; the strip has to follow.
(function () {
  var report = function (on) {
    window.__TAURI_INTERNALS__.invoke("page_fullscreen", { on: on }).catch(function () {});
  };

  document.addEventListener("fullscreenchange", function () {
    report(Boolean(document.fullscreenElement));
  });

  // Safari style prefix, still used by some players.
  document.addEventListener("webkitfullscreenchange", function () {
    report(Boolean(document.webkitFullscreenElement));
  });
})();

// WebView2 renders its own error page; detect it so we can show ours instead.
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

// Scroll position is remembered per tab so switching back does not jump to top.
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

// While the address bar owns the keyboard, keys are forwarded to it. This does
// not depend on which webview has OS focus.
(function () {
  var forward = function (kind, value) {
    var payload = kind === "type" ? { text: value } : { key: value };
    var command = kind === "type" ? "url_type" : "url_key";
    window.__TAURI_INTERNALS__.invoke(command, payload).catch(function () {});
  };

  // An input method only composes inside an editable element, and in url mode the
  // page holds the keyboard but has none. A hidden field collects the
  // composition and its final text is forwarded as one insertion.
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

  // Editing keys the address bar has to act on itself: the browser default that
  // would have done it belongs to the page, which currently has the keyboard.
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
    // A composition owns its keystrokes: forwarding the space that picks a
    // candidate, or the backspace that edits one, would break the input method.
    if (event.isComposing || event.keyCode === 229) return;
    // preventDefault stops the character, stopPropagation stops the page's own
    // single-key shortcuts (GitHub's t, YouTube's k) from firing while the
    // address bar is being typed into.
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
    if (event.key === "F5") return "reload";
    if (event.key === "F11") return "fullscreen";
    if (event.key === "F12") return "devtools";
    var ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) return null;
    // Shift turns "t" into "T"; the switch is keyed on the lower-case letter.
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
      // Plain Ctrl+A must stay select-all, so only the shifted form is ours.
      case "a": return event.shiftKey ? "tab_search" : null;
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

  // Suppress the native menu and let the shell draw its own.
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

  // Any click in the page closes an open menu, but only for a short window
  // after one was requested, so ordinary clicks stay IPC free.
  var menuShownAt = 0;
  window.addEventListener("mousedown", function () {
    if (Date.now() - menuShownAt > 8000) return;
    menuShownAt = 0;
    report({ dismiss: true });
  }, true);

  // Sensitive APIs ask the shell first; the prompt is ours, not a browser dialog.
  // The command answers at once - either with a remembered decision or with a
  // prompt id - and the answer is collected here so no ipc thread is held while
  // the user decides.
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
    return invoke("permission_request", {
      origin: location.origin,
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

  window.addEventListener("keydown", function (event) {
    if (event.altKey && event.shiftKey && event.key.indexOf("Arrow") === 0 && window.__TAURI_INTERNALS__) {
      event.preventDefault();
      window.__TAURI_INTERNALS__.invoke("snap", {
        direction: event.key.replace("Arrow", "").toLowerCase()
      }).catch(function () {});
      return;
    }
    var action = actionFor(event);
    if (!action) return;
    event.preventDefault();
    event.stopPropagation();
    send(action);
  }, true);
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
    // Scrolling stays in the page: no round trip needed.
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
    // Many sites wrap cards in an <a> without href and navigate from JS.
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

  // Exactly one source: a link, an image, or the selected text.
  /// Shadow DOM retargets events to the host, so the real link only shows up
  /// in composedPath().
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

  /// Sites often stack a container above the cards; the stack still holds them.
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
      // No address on the element: treat the label as the drag source.
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

    // Sites that build cards from divs offer no address, but they do look
    // clickable; the visible text is still worth dragging.
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
    // Nothing is intercepted here. A plain click has to reach the page: taking
    // the pointer and cancelling the default on press stopped the site's own
    // click handler from ever running, so a card that opened a tab on click
    // only flashed the drag badge instead.
  }, true);

  window.addEventListener('dragstart', function (event) {
    // Native drag would cancel the pointer stream.
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
      // The gesture is a drag from here on: keep the pointer stream so the page
      // does not start a native drag, and stop text selection under the cursor.
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
    // One action per drag, even if several events arrive.
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
    // "open": a link or image opens, text searches.
    if (url) invoke('drag_action', { kind: 'open', url: url });
    else if (text) invoke('drag_action', { kind: 'search', text: text });
  }

  window.addEventListener('pointerup', finish, true);

  // Some WebView2 builds drop pointer events for native drags; the mouse pair
  // is a cheap safety net and shares the same state.
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

