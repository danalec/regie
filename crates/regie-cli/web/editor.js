/*
 * The editor of the workbench.
 *
 * The document is shown in three layers that have to agree with each other:
 * the gutter with the line numbers, the minimap on the right, and the
 * highlighted text behind the transparent textarea.  This module owns the
 * three of them, the toolbar, the find bar and the save button.
 *
 * The script is concatenated with other modules into a single file, so
 * everything lives in a closure and only the controller reaches the window.
 */
(function () {
  'use strict';

  /* The geometry of the editor, in pixels.
   *
   * These are the values of the custom properties --row and --pad of the
   * stylesheet.  The band, the gutter and the minimap are placed with them,
   * so a change in the stylesheet has to be mirrored here.
   */
  const ROW = 22;
  const PAD = 10;

  /* The line of a text offset and where that line starts.
   *
   * The line is zero-based; the callers that show a position to the user
   * add one.
   */
  function scanOffset(text, offset) {
    const limit = Math.max(0, Math.min(offset, text.length));
    let line = 0;
    let start = 0;
    for (let index = 0; index < limit; index += 1) {
      if (text.charCodeAt(index) === 10) {
        line += 1;
        start = index + 1;
      }
    }
    return { line: line, start: start };
  }

  /* The line of a text offset, zero-based. */
  function lineIndexOf(text, offset) {
    return scanOffset(text, offset).line;
  }

  /* The position of a text offset: the line and the column, both one-based. */
  function positionOf(text, offset) {
    const scan = scanOffset(text, offset);
    return { line: scan.line + 1, column: offset - scan.start + 1 };
  }

  /* The first character of every line, in order. */
  function lineStarts(text) {
    const starts = [0];
    for (let index = 0; index < text.length; index += 1) {
      if (text.charCodeAt(index) === 10) starts.push(index + 1);
    }
    return starts;
  }

  /* The line of an offset, given the starts of the lines, one-based. */
  function lineOfOffset(starts, offset) {
    let low = 0;
    let high = starts.length - 1;
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (starts[middle] <= offset) low = middle;
      else high = middle - 1;
    }
    return low + 1;
  }

  /* The offset of a line and a column, both one-based. */
  function offsetOf(text, line, column) {
    const starts = lineStarts(text);
    const index = Math.max(0, Math.min(line - 1, starts.length - 1));
    const start = starts[index];
    const end = index + 1 < starts.length ? starts[index + 1] - 1 : text.length;
    const place = Math.max(1, Math.min(column, end - start + 1));
    return start + place - 1;
  }

  /* The character offset of a byte offset.
   *
   * The engine counts bytes and the editor counts characters, so the text is
   * encoded once and the two are walked together.  A byte offset that lands
   * inside a multi-byte character rounds down to the start of that character,
   * which is the only sensible reading of half a character.
   */
  function characterOf(text, bytes, byteOffset) {
    if (byteOffset <= 0) return 0;
    if (byteOffset >= bytes.length) return text.length;
    let byteCursor = 0;
    let charCursor = 0;
    while (charCursor < text.length) {
      const point = text.codePointAt(charCursor);
      const size = point < 0x80 ? 1 : point < 0x800 ? 2 : point < 0x10000 ? 3 : 4;
      if (byteCursor + size > byteOffset) return charCursor;
      byteCursor += size;
      charCursor += point > 0xffff ? 2 : 1;
    }
    return text.length;
  }

  /* The number of occurrences of a needle in a text. */
  function countOf(text, needle) {
    if (!needle) return 0;
    let count = 0;
    let index = text.indexOf(needle);
    while (index >= 0) {
      count += 1;
      index = text.indexOf(needle, index + needle.length);
    }
    return count;
  }

  /* The change of a heading line.
   *
   * The line loses the heading it had and gains the one that was asked for,
   * so the key turns a heading into another heading instead of stacking two
   * markers on the same line.
   */
  function headingTransform(marker) {
    return function (line) {
      const stripped = line.replace(/^ {0,3}#{1,6}([ \t]+|$)/, '');
      return {
        text: marker + stripped,
        removed: line.length - stripped.length,
        added: marker.length
      };
    };
  }

  /* The change of a prefixed line, for the quotes and the lists. */
  function prefixTransform(prefixFor) {
    return function (line, index) {
      const prefix = prefixFor(index);
      return { text: prefix + line, removed: 0, added: prefix.length };
    };
  }

  /* The editor.
   *
   * The options are the five elements of the editor and the two callbacks.
   * The controller answers the text, the caret and the match of the engine.
   */
  function create(options) {
    const opts = options || {};
    const textarea = opts.textarea;
    const gutter = opts.gutter;
    const highlight = opts.highlight;
    const band = opts.band;
    const strip = opts.strip;

    if (!textarea || !gutter || !highlight || !band || !strip) {
      throw new Error('regie editor: the editor needs a textarea, a gutter, a highlight, a band and a strip');
    }

    const onChange = typeof opts.onChange === 'function' ? opts.onChange : function () {};
    const onCaret = typeof opts.onCaret === 'function' ? opts.onCaret : function () {};

    const doc = textarea.ownerDocument || document;
    const view = doc.defaultView || window;

    /* The elements of the toolbar and of the find bar.
     *
     * They are looked up by their id in the document of the textarea, so the
     * editor works with the page as it is written, and a page without one of
     * them simply does without that part.
     */
    const toolbar = doc.getElementById('toolbar');
    const findBar = doc.getElementById('find');
    const findToggle = doc.getElementById('find-toggle');
    const findInput = doc.getElementById('find-text');
    const replaceInput = doc.getElementById('replace-text');
    const findNextButton = doc.getElementById('find-next');
    const findReplaceButton = doc.getElementById('find-replace');
    const findAllButton = doc.getElementById('find-all');
    const findStatus = doc.getElementById('find-status');
    const copyButton = doc.getElementById('copy');
    const saveButton = doc.getElementById('save');

    /* The state of the editor. */
    let caret = { line: 1, column: 1 };
    let matchStart = -1;
    let matchEnd = -1;

    /* The label of the copy button, and the timer that puts it back. */
    const copyLabel = copyButton ? copyButton.textContent : 'copy md';
    let copyTimer = null;

    /* The rows of the gutter and the ticks of the minimap, one per line. */
    let rows = [];
    let ticks = [];
    let rowCount = -1;
    let caretRow = null;
    let hitRow = null;
    let hitTick = null;

    /* ---- the layers ---- */

    /* Builds the rows of the gutter and the ticks of the minimap.
     *
     * The two are only rebuilt when the number of lines changes; a caret
     * that moves inside the document only changes the classes of the rows.
     */
    function buildRows(count) {
      const numbers = doc.createDocumentFragment();
      const marks = doc.createDocumentFragment();
      rows = [];
      ticks = [];
      for (let line = 1; line <= count; line += 1) {
        const row = doc.createElement('b');
        row.textContent = String(line);
        rows.push(row);
        numbers.appendChild(row);
        const tick = doc.createElement('i');
        ticks.push(tick);
        marks.appendChild(tick);
      }
      gutter.replaceChildren(numbers);
      strip.replaceChildren(marks);
      rowCount = count;
      caretRow = null;
      hitRow = null;
      hitTick = null;
    }

    /* Marks the line that holds the caret. */
    function markCaret() {
      const row = rows[caret.line - 1] || null;
      if (row === caretRow) return;
      if (caretRow) caretRow.classList.remove('now');
      caretRow = row;
      if (caretRow) caretRow.classList.add('now');
    }

    /* Marks the line that holds the match. */
    function markHit() {
      const text = textarea.value;
      const starts = lineStarts(text);
      const line = matchStart >= 0 ? lineOfOffset(starts, matchStart) : 0;
      const row = rows[line - 1] || null;
      if (row !== hitRow) {
        if (hitRow) hitRow.classList.remove('hit');
        hitRow = row;
        if (hitRow) hitRow.classList.add('hit');
      }
      const tick = ticks[line - 1] || null;
      if (tick !== hitTick) {
        if (hitTick) hitTick.classList.remove('hit');
        hitTick = tick;
        if (hitTick) hitTick.classList.add('hit');
      }
    }

    /* Writes the highlighted text.
     *
     * The markdown module escapes the document and marks the tokens, so the
     * only markup in the layer is the one the module built.
     */
    function renderHighlight() {
      const text = textarea.value;
      const module = view.RegieMarkdown;
      if (!module || typeof module.highlight !== 'function') {
        highlight.textContent = text;
        return;
      }
      if (matchStart >= 0) {
        highlight.innerHTML = module.highlight(text, matchStart, matchEnd);
      } else {
        highlight.innerHTML = module.highlight(text);
      }
    }

    /* Places the band on the line of the caret.
     *
     * The band is a child of the editor, so its top is measured from the top
     * of the text: the padding of the layer, the lines above the caret, and
     * the scroll of the textarea.
     */
    function updateBand() {
      band.style.top = (PAD + (caret.line - 1) * ROW - textarea.scrollTop) + 'px';
      band.style.display = 'block';
    }

    /* Keeps the three layers on the same part of the document.
     *
     * The highlighted text sits behind the textarea and has its own scroll
     * offsets, so both of them have to follow the textarea; the gutter has
     * no horizontal scroll, so it only follows the vertical one.  The
     * minimap is shorter than the text, so it scrolls at the rate of its own
     * height.
     */
    function syncScrolls() {
      highlight.scrollTop = textarea.scrollTop;
      highlight.scrollLeft = textarea.scrollLeft;
      gutter.scrollTop = textarea.scrollTop;
      const stripRange = strip.scrollHeight - strip.clientHeight;
      const textRange = textarea.scrollHeight - textarea.clientHeight;
      strip.scrollTop = stripRange > 0 && textRange > 0
        ? textarea.scrollTop * stripRange / textRange
        : 0;
    }

    /* Draws everything that follows from the text and the match. */
    function render() {
      const starts = lineStarts(textarea.value);
      if (starts.length !== rowCount) buildRows(starts.length);
      markCaret();
      markHit();
      renderHighlight();
      updateBand();
      syncScrolls();
    }

    /* ---- the text ---- */

    /* Replaces the document and tells the world about it.
     *
     * The match belongs to the text that was there, so it is dropped: the
     * engine answers with a new one as soon as it has one.
     */
    function apply(text, selStart, selEnd) {
      const value = typeof text === 'string' ? text : '';
      const limit = value.length;
      const start = Math.max(0, Math.min(selStart, limit));
      const end = Math.max(start, Math.min(selEnd, limit));
      textarea.value = value;
      textarea.setSelectionRange(start, end);
      matchStart = -1;
      matchEnd = -1;
      caret = positionOf(value, start);
      render();
      onChange(value);
      onCaret(caret.line, caret.column);
    }

    /* Brings a line into view in the textarea. */
    function revealLine(line) {
      const top = PAD + (line - 1) * ROW;
      const height = textarea.clientHeight;
      if (top < textarea.scrollTop) {
        textarea.scrollTop = top;
      } else if (top + ROW > textarea.scrollTop + height) {
        textarea.scrollTop = top + ROW - height;
      }
    }

    /* The place of a column inside a line, in offsets. */
    function placeInLine(text, starts, line, column) {
      const start = starts[line];
      const end = line + 1 < starts.length ? starts[line + 1] - 1 : text.length;
      const place = Math.max(0, Math.min(column, end - start));
      return start + place;
    }

    /* Changes a block of lines and keeps the caret where it belongs.
     *
     * The transform answers the new text of a line together with the number
     * of characters it removed and added at the start of the line; the caret
     * keeps its place in the line and follows the change.
     */
    function blockEdit(text, selStart, selEnd, transform) {
      const lines = text.split('\n');
      const starts = lineStarts(text);
      const first = lineIndexOf(text, selStart);
      const last = selEnd > selStart ? lineIndexOf(text, selEnd - 1) : first;
      const caretColumn = selStart - starts[first];
      const endColumn = selEnd - starts[last];

      let caretShift = 0;
      let endShift = 0;
      for (let index = first; index <= last; index += 1) {
        const result = transform(lines[index], index - first);
        lines[index] = result.text;
        const shift = result.added - result.removed;
        if (index === first) caretShift = shift;
        if (index === last) endShift = shift;
      }

      const value = lines.join('\n');
      const newStarts = lineStarts(value);
      const caretOffset = placeInLine(value, newStarts, first, caretColumn + caretShift);
      const endOffset = placeInLine(value, newStarts, last, endColumn + endShift);
      apply(value, Math.min(caretOffset, endOffset), Math.max(caretOffset, endOffset));
    }

    /* ---- the toolbar ---- */

    /* Wraps the selection in a marker. */
    function wrapEdit(text, selStart, selEnd, marker) {
      const chosen = text.slice(selStart, selEnd);
      const value = text.slice(0, selStart) + marker + chosen + marker + text.slice(selEnd);
      const start = selStart + marker.length;
      apply(value, start, start + chosen.length);
    }

    /* Writes a link around the selection.
     *
     * With a selection the text of the link is the selection and it keeps it;
     * without one the template is written and the url is preselected, ready
     * to be typed over.
     */
    function linkEdit(text, selStart, selEnd) {
      const chosen = selEnd > selStart ? text.slice(selStart, selEnd) : 'text';
      const inserted = '[' + chosen + '](url)';
      const value = text.slice(0, selStart) + inserted + text.slice(selEnd);
      if (selEnd > selStart) {
        const start = selStart + 1;
        apply(value, start, start + chosen.length);
      } else {
        const start = selStart + inserted.length - 4;
        apply(value, start, start + 3);
      }
    }

    /* Writes a table with a header row and a separator row. */
    function tableEdit(text, selStart) {
      const table = '| Column 1 | Column 2 |\n| --- | --- |\n';
      const before = text.slice(0, selStart);
      const after = text.slice(selStart);
      const lead = before.length > 0 && before.charAt(before.length - 1) !== '\n' ? '\n' : '';
      const value = before + lead + table + after;
      const start = selStart + lead.length + table.length;
      apply(value, start, start);
    }

    /* The action of a button of the toolbar. */
    function markdownAction(kind) {
      const text = textarea.value;
      const selStart = textarea.selectionStart;
      const selEnd = textarea.selectionEnd;
      switch (kind) {
        case 'h1':
          blockEdit(text, selStart, selEnd, headingTransform('# '));
          break;
        case 'h2':
          blockEdit(text, selStart, selEnd, headingTransform('## '));
          break;
        case 'bold':
          wrapEdit(text, selStart, selEnd, '**');
          break;
        case 'italic':
          wrapEdit(text, selStart, selEnd, '*');
          break;
        case 'code':
          wrapEdit(text, selStart, selEnd, '`');
          break;
        case 'link':
          linkEdit(text, selStart, selEnd);
          break;
        case 'quote':
          blockEdit(text, selStart, selEnd, prefixTransform(function () { return '> '; }));
          break;
        case 'ul':
          blockEdit(text, selStart, selEnd, prefixTransform(function () { return '- '; }));
          break;
        case 'ol':
          blockEdit(text, selStart, selEnd, prefixTransform(function (index) { return (index + 1) + '. '; }));
          break;
        case 'table':
          tableEdit(text, selStart);
          break;
        default:
          return;
      }
      textarea.focus();
    }

    if (toolbar) {
      /* The buttons take the focus away from the textarea, and a button that
       * takes the focus may take the selection with it; the mousedown is
       * stopped so the selection of the document survives the click.
       */
      toolbar.addEventListener('mousedown', function (event) {
        if (event.target && event.target.tagName === 'BUTTON') event.preventDefault();
      });
      toolbar.addEventListener('click', function (event) {
        const button = event.target && event.target.closest
          ? event.target.closest('button[data-md]')
          : null;
        if (!button) return;
        event.preventDefault();
        markdownAction(button.getAttribute('data-md'));
      });
    }

    /* ---- the find bar ---- */

    /* The number of occurrences of the needle, written in the status. */
    function updateStatus() {
      if (!findStatus) return;
      const needle = findInput ? findInput.value : '';
      if (!needle) {
        findStatus.textContent = '';
        return;
      }
      const count = countOf(textarea.value, needle);
      findStatus.textContent = count === 0
        ? 'no match'
        : count === 1 ? '1 match' : count + ' matches';
    }

    /* Writes a message in the status of the find bar. */
    function setStatus(message) {
      if (findStatus) findStatus.textContent = message;
    }

    /* Finds the next occurrence of the needle and selects it. */
    function findNext() {
      const needle = findInput ? findInput.value : '';
      if (!needle) {
        setStatus('');
        return;
      }
      const text = textarea.value;
      let index = text.indexOf(needle, textarea.selectionEnd);
      if (index < 0) index = text.indexOf(needle, 0);
      if (index < 0) {
        setStatus('no match');
        return;
      }
      textarea.setSelectionRange(index, index + needle.length);
      caret = positionOf(text, index);
      markCaret();
      revealLine(caret.line);
      updateStatus();
      onCaret(caret.line, caret.column);
    }

    /* Replaces the selection and finds the next occurrence. */
    function replaceCurrent() {
      const needle = findInput ? findInput.value : '';
      const replacement = replaceInput ? replaceInput.value : '';
      if (!needle) {
        updateStatus();
        return;
      }
      const selStart = textarea.selectionStart;
      const selEnd = textarea.selectionEnd;
      if (selEnd > selStart) {
        const text = textarea.value;
        const value = text.slice(0, selStart) + replacement + text.slice(selEnd);
        const start = selStart + replacement.length;
        textarea.value = value;
        textarea.setSelectionRange(start, start);
        matchStart = -1;
        matchEnd = -1;
        caret = positionOf(value, start);
        render();
        onChange(value);
        onCaret(caret.line, caret.column);
      }
      findNext();
    }

    /* Replaces every occurrence of the needle. */
    function replaceAll() {
      const needle = findInput ? findInput.value : '';
      const replacement = replaceInput ? replaceInput.value : '';
      if (!needle) {
        updateStatus();
        return;
      }
      const text = textarea.value;
      const parts = text.split(needle);
      const count = parts.length - 1;
      if (count === 0) {
        setStatus('no match');
        return;
      }
      const value = parts.join(replacement);
      const start = Math.min(textarea.selectionStart, value.length);
      textarea.value = value;
      textarea.setSelectionRange(start, start);
      matchStart = -1;
      matchEnd = -1;
      caret = positionOf(value, start);
      render();
      onChange(value);
      onCaret(caret.line, caret.column);
      setStatus('replaced ' + count);
    }

    if (findToggle && findBar) {
      findToggle.addEventListener('click', function (event) {
        event.preventDefault();
        const open = findBar.classList.toggle('on');
        if (open && findInput) {
          findInput.focus();
          findInput.select();
        } else {
          textarea.focus();
        }
      });
    }
    if (findInput) {
      findInput.addEventListener('input', updateStatus);
      findInput.addEventListener('keydown', function (event) {
        if (event.key === 'Enter') {
          event.preventDefault();
          findNext();
        }
      });
    }
    if (findNextButton) {
      findNextButton.addEventListener('click', function (event) {
        event.preventDefault();
        findNext();
      });
    }
    if (findReplaceButton) {
      findReplaceButton.addEventListener('click', function (event) {
        event.preventDefault();
        replaceCurrent();
      });
    }
    if (findAllButton) {
      findAllButton.addEventListener('click', function (event) {
        event.preventDefault();
        replaceAll();
      });
    }

    /* ---- the copy button ---- */

    if (copyButton) {
      copyButton.addEventListener('click', function (event) {
        event.preventDefault();
        copyMarkdown(textarea.value);
      });
    }

    /* Puts the document on the clipboard.
     *
     * The clipboard of the browser is only there in a secure context, and a
     * page that is not one still copies through the old command.  Either way
     * the button says what happened.
     */
    function copyMarkdown(text) {
      const clipboard = view.navigator && view.navigator.clipboard;
      if (clipboard && typeof clipboard.writeText === 'function') {
        clipboard.writeText(text).then(
          function () { flashCopy('copied'); },
          function () { flashCopy(legacyCopy(text) ? 'copied' : 'failed'); }
        );
        return;
      }
      flashCopy(legacyCopy(text) ? 'copied' : 'failed');
    }

    /* The old way of copying, for a page that is not a secure context. */
    function legacyCopy(text) {
      const area = doc.createElement('textarea');
      area.value = text;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.top = '-1000px';
      doc.body.appendChild(area);
      area.select();
      let done = false;
      try {
        done = doc.execCommand('copy');
      } catch (error) {
        done = false;
      }
      area.remove();
      return done;
    }

    /* Shows the outcome of the copy on the button, for a moment. */
    function flashCopy(message) {
      if (!copyButton) return;
      copyButton.textContent = message;
      copyButton.classList.add('flash');
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(function () {
        copyButton.textContent = copyLabel;
        copyButton.classList.remove('flash');
      }, 1200);
    }

    /* ---- the save button ---- */

    if (saveButton) {
      saveButton.addEventListener('click', function (event) {
        event.preventDefault();
        const urls = view.URL || URL;
        const blob = new Blob([textarea.value], { type: 'text/markdown;charset=utf-8' });
        const url = urls.createObjectURL(blob);
        const anchor = doc.createElement('a');
        anchor.href = url;
        anchor.download = 'document.md';
        anchor.style.display = 'none';
        doc.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
        setTimeout(function () { urls.revokeObjectURL(url); }, 1000);
      });
    }

    /* ---- the events of the textarea ---- */

    function caretMoved() {
      const position = positionOf(textarea.value, textarea.selectionStart);
      if (position.line === caret.line && position.column === caret.column) return;
      caret = position;
      markCaret();
      updateBand();
      onCaret(caret.line, caret.column);
    }

    textarea.addEventListener('input', function () {
      const text = textarea.value;
      matchStart = -1;
      matchEnd = -1;
      caret = positionOf(text, textarea.selectionStart);
      render();
      onChange(text);
      onCaret(caret.line, caret.column);
    });
    textarea.addEventListener('keyup', caretMoved);
    textarea.addEventListener('mouseup', caretMoved);
    textarea.addEventListener('focus', caretMoved);
    textarea.addEventListener('select', caretMoved);
    textarea.addEventListener('scroll', function () {
      updateBand();
      syncScrolls();
    });
    doc.addEventListener('selectionchange', function (event) {
      if (event.target === textarea) caretMoved();
    });

    /* ---- the controller ---- */

    const controller = {
      /* The document. */
      getText: function () {
        return textarea.value;
      },

      /* Replaces the document and draws it again. */
      setText: function (text) {
        const value = typeof text === 'string' ? text : '';
        textarea.value = value;
        textarea.setSelectionRange(0, 0);
        matchStart = -1;
        matchEnd = -1;
        caret = { line: 1, column: 1 };
        render();
        onChange(value);
        onCaret(caret.line, caret.column);
      },

      /* The match of the engine.
       *
       * The engine counts bytes and the editor counts characters, so the two
       * offsets are converted before the match is drawn.
       */
      setMatch: function (byteStart, byteEnd) {
        if (byteStart === null || byteStart === undefined ||
            byteEnd === null || byteEnd === undefined) {
          matchStart = -1;
          matchEnd = -1;
          renderHighlight();
          markHit();
          return;
        }
        const text = textarea.value;
        const bytes = new TextEncoder().encode(text);
        let start = characterOf(text, bytes, byteStart);
        let end = characterOf(text, bytes, byteEnd);
        if (end < start) {
          const swap = start;
          start = end;
          end = swap;
        }
        matchStart = start;
        matchEnd = end;
        renderHighlight();
        markHit();
      },

      /* The caret. */
      focus: function () {
        textarea.focus();
      },

      /* The position of the caret, one-based. */
      getCaret: function () {
        return { line: caret.line, column: caret.column };
      },

      /* Moves the caret to a line and a column, one-based. */
      setCaret: function (line, column) {
        const text = textarea.value;
        const offset = offsetOf(text, line, column);
        textarea.setSelectionRange(offset, offset);
        caret = positionOf(text, offset);
        markCaret();
        updateBand();
        onCaret(caret.line, caret.column);
      }
    };

    caret = positionOf(textarea.value, textarea.selectionStart);
    render();
    return controller;
  }

  window.RegieEditor = { create: create };
})();
