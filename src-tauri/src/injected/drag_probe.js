(function () {
  var old = document.getElementById('__mix_drag_probe');
  if (old) old.remove();
  var box = document.createElement('div');
  box.id = '__mix_drag_probe';
  box.style.cssText =
    'position:fixed;left:12px;top:12px;z-index:2147483647;max-width:80%;padding:10px 12px;' +
    'background:#202124f2;color:#e8eaed;font:11px monospace;border:1px solid #4c8dff;border-radius:8px;white-space:pre-wrap;pointer-events:none;';
  box.textContent = 'drag probe: press anywhere';
  document.documentElement.appendChild(box);

  function brief(node) {
    if (!node || node.nodeType !== 1) return '?';
    var name = node.tagName;
    var cls = String(node.className || '').split(' ').filter(Boolean).slice(0, 2).join('.');
    var anchor = node.closest ? node.closest('a') : null;
    var mark = anchor ? (anchor.href ? 'A:' + anchor.href.slice(8, 34) : 'A(nohref)') : '';
    return name + (cls ? '.' + cls : '') + (mark ? ' [' + mark + ']' : '');
  }

  document.addEventListener('pointerdown', function (event) {
    var stack = [];
    try { stack = document.elementsFromPoint(event.clientX, event.clientY) || []; } catch (error) {}
    var lines = ['drag probe', 'target=' + brief(event.target), 'stack=' + stack.length];
    for (var i = 0; i < stack.length && i < 6; i += 1) {
      lines.push('  ' + i + ' ' + brief(stack[i]));
    }
    var anyAnchor = false;
    for (var j = 0; j < stack.length; j += 1) {
      if (stack[j].closest && stack[j].closest('a')) { anyAnchor = true; break; }
    }
    lines.push('anchor in stack=' + anyAnchor);
    box.textContent = lines.join('\n');
  }, true);

  setTimeout(function () { if (box.parentNode) box.parentNode.removeChild(box); }, 40000);
})();
