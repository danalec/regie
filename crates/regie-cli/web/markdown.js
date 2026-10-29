/* The Markdown layer of the workbench.
 *
 * The page shows the same document twice: as a preview, and as a source
 * editor with the Markdown marked up.  This file serves both.
 *
 *   render(text)                the preview, as an HTML string
 *   highlight(text, start, end)  the source, as an HTML string, with the
 *                                Markdown syntax marked up and the matched
 *                                range wrapped
 *
 * The highlighter is the delicate half.  Its output is laid under a
 * transparent textarea, so the two have to agree on every character: what
 * the highlighter emits, once the markup is taken away, is the input again,
 * newlines included.  The renderer has no such constraint and is free to
 * drop the Markdown syntax.
 *
 * There are no dependencies and no build step: the server concatenates this
 * file with the other parts of the page, so everything lives inside one
 * function and nothing leaks into the global scope.
 */
(function () {
  'use strict';

  /* ---------- escaping ---------- */

  var HTML_ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' };

  /**
   * Escapes the characters that would otherwise be read as markup.
   *
   * @param {string} text the text to escape
   * @returns {string} the text, safe to put in the page
   */
  function escapeHtml(text) {
    return String(text).replace(/[&<>"]/g, function (character) {
      return HTML_ESCAPES[character];
    });
  }

  /**
   * Keeps a URL that would run script out of the page.
   *
   * The scheme is read after the whitespace and the control characters are
   * taken out, because a browser takes them out too before it decides what
   * the scheme is.
   *
   * @param {string} url the URL of a link or of an image
   * @returns {string} the URL, or '#' when the URL would run script
   */
  function safeUrl(url) {
    var probe = '';
    for (var index = 0; index < url.length; index += 1) {
      var code = url.charCodeAt(index);
      if (code > 32 && code !== 127) {
        probe += url.charAt(index);
      }
    }
    probe = probe.toLowerCase();
    if (probe.indexOf('javascript:') === 0 || probe.indexOf('vbscript:') === 0) {
      return '#';
    }
    return String(url);
  }

  /* ---------- the lines of a document ---------- */

  /**
   * Cuts a document into its lines, keeping the line endings.
   *
   * @param {string} source the whole document
   * @returns {Array} the lines, each with its text, its ending and the
   *                  offset of its first character
   */
  function splitLines(source) {
    var lines = [];
    var start = 0;
    var index = 0;
    while (index < source.length) {
      if (source.charAt(index) === '\n') {
        lines.push({ text: source.substring(start, index), terminator: '\n', offset: start });
        start = index + 1;
      }
      index += 1;
    }
    if (start < source.length) {
      lines.push({ text: source.substring(start), terminator: '', offset: start });
    }
    return lines;
  }

  /**
   * Takes the carriage return off the end of a line.
   *
   * @param {string} text the text of a line
   * @returns {string} the text without a trailing carriage return
   */
  function withoutCarriageReturn(text) {
    return text.charAt(text.length - 1) === '\r' ? text.substring(0, text.length - 1) : text;
  }

  /* ---------- the inline scanner ---------- */

  var ESCAPABLE = /[!"#$%&'()*+,\-.\/:;<=>?@[\\\]^_`{|}~]/;

  /**
   * Reads the inline markup of one line.
   *
   * The nodes keep the delimiters they came with, so the highlighter can
   * put them back where they were.
   *
   * @param {string} text the line
   * @returns {Array} the nodes of the line
   */
  function inlineNodes(text) {
    var nodes = [];
    var buffer = '';
    var index = 0;

    function flush() {
      if (buffer !== '') {
        nodes.push({ type: 'text', text: buffer });
        buffer = '';
      }
    }

    while (index < text.length) {
      var character = text.charAt(index);

      if (character === '\\' && index + 1 < text.length && ESCAPABLE.test(text.charAt(index + 1))) {
        flush();
        nodes.push({ type: 'escape', raw: character + text.charAt(index + 1), text: text.charAt(index + 1) });
        index += 2;
        continue;
      }

      if (character === '`') {
        var code = codeSpan(text, index);
        if (code !== null) {
          flush();
          nodes.push(code);
          index = code.at;
          continue;
        }
      }

      if (character === '!' && text.charAt(index + 1) === '[') {
        var image = imageNode(text, index);
        if (image !== null) {
          flush();
          nodes.push(image);
          index = image.at;
          continue;
        }
      }

      if (character === '[') {
        var link = linkNode(text, index);
        if (link !== null) {
          flush();
          nodes.push(link);
          index = link.at;
          continue;
        }
      }

      if (character === '*' || character === '_') {
        var emphasis = emphasisNode(text, index, character);
        if (emphasis !== null) {
          flush();
          nodes.push(emphasis);
          index = emphasis.at;
          continue;
        }
      }

      buffer += character;
      index += 1;
    }

    flush();
    return nodes;
  }

  /**
   * Reads a code span.
   *
   * @param {string} text the line
   * @param {number} index where the opening backtick run starts
   * @returns {Object} the node, or null when the span is not closed
   */
  function codeSpan(text, index) {
    var open = 0;
    while (text.charAt(index + open) === '`') {
      open += 1;
    }
    var contentStart = index + open;
    var cursor = contentStart;
    while (cursor < text.length) {
      if (text.charAt(cursor) === '`') {
        var close = 0;
        while (text.charAt(cursor + close) === '`') {
          close += 1;
        }
        if (close === open) {
          var raw = text.substring(contentStart, cursor);
          var body = raw;
          if (body.length >= 2 && body.charAt(0) === ' ' && body.charAt(body.length - 1) === ' ') {
            body = body.substring(1, body.length - 1);
          }
          return {
            type: 'code',
            mark: text.substring(index, contentStart),
            raw: raw,
            text: body,
            at: cursor + close
          };
        }
        cursor += close;
        continue;
      }
      cursor += 1;
    }
    return null;
  }

  /**
   * Finds the parenthesis that ends a URL.
   *
   * The parentheses of the URL are counted, so a URL that holds a pair of
   * them is read whole.
   *
   * @param {string} text the line
   * @param {number} start where the URL starts
   * @returns {number} the place of the parenthesis, or -1 when there is none
   */
  function closingParenthesis(text, start) {
    var depth = 0;
    for (var index = start; index < text.length; index += 1) {
      var character = text.charAt(index);
      if (character === '\\') {
        index += 1;
        continue;
      }
      if (character === '(') {
        depth += 1;
        continue;
      }
      if (character === ')') {
        if (depth === 0) {
          return index;
        }
        depth -= 1;
      }
    }
    return -1;
  }

  /**
   * Reads an image.
   *
   * @param {string} text the line
   * @param {number} index where the bang starts
   * @returns {Object} the node, or null when the image is not complete
   */
  function imageNode(text, index) {
    var close = text.indexOf(']', index + 2);
    if (close < 0 || text.charAt(close + 1) !== '(') {
      return null;
    }
    var end = closingParenthesis(text, close + 2);
    if (end < 0) {
      return null;
    }
    return {
      type: 'image',
      mark: '![',
      close: '](',
      url: text.substring(close + 2, end),
      alt: text.substring(index + 2, close),
      at: end + 1
    };
  }

  /**
   * Reads a link.
   *
   * @param {string} text the line
   * @param {number} index where the bracket starts
   * @returns {Object} the node, or null when the link is not complete
   */
  function linkNode(text, index) {
    var close = text.indexOf(']', index + 1);
    if (close < 0 || text.charAt(close + 1) !== '(') {
      return null;
    }
    var end = closingParenthesis(text, close + 2);
    if (end < 0) {
      return null;
    }
    var label = text.substring(index + 1, close);
    return {
      type: 'link',
      mark: '[',
      close: '](',
      url: text.substring(close + 2, end),
      children: inlineNodes(label),
      at: end + 1
    };
  }

  /**
   * Reads bold or italic text.
   *
   * Two stars are bold and two underscores are bold as well; a single star
   * or a single underscore is italic.  An underscore inside a word is left
   * alone, so `snake_case_names` stays a word.
   *
   * @param {string} text the line
   * @param {number} index where the marker starts
   * @param {string} marker the marker character
   * @returns {Object} the node, or null when there is no closing marker
   */
  function emphasisNode(text, index, marker) {
    var run = 0;
    while (text.charAt(index + run) === marker) {
      run += 1;
    }
    if (run === 0) {
      return null;
    }
    var bold = run >= 2;
    var delimiter = bold ? marker + marker : marker;

    if (marker === '_') {
      var before = index > 0 ? text.charAt(index - 1) : '';
      if (before !== '' && /[0-9A-Za-z]/.test(before)) {
        return null;
      }
    }

    var contentStart = index + delimiter.length;
    var cursor = contentStart;
    while (cursor < text.length) {
      if (text.charAt(cursor) === '\\') {
        cursor += 2;
        continue;
      }
      if (text.substr(cursor, delimiter.length) === delimiter) {
        if (marker === '_') {
          var after = text.charAt(cursor + delimiter.length);
          if (after !== '' && /[0-9A-Za-z]/.test(after)) {
            cursor += 1;
            continue;
          }
        }
        var raw = text.substring(contentStart, cursor);
        if (raw === '') {
          cursor += 1;
          continue;
        }
        return {
          type: bold ? 'bold' : 'italic',
          mark: delimiter,
          children: inlineNodes(raw),
          at: cursor + delimiter.length
        };
      }
      cursor += 1;
    }
    return null;
  }

  /* ---------- the lines of a document, classified ---------- */

  var HEADING = /^(#{1,6})([ \t]+)(.*)$/;
  var RULE = /^([ \t]{0,3})([-*_])[ \t]*(?:\2[ \t]*){2,}$/;
  var QUOTE = /^([ \t]{0,3}>[ \t]?)(.*)$/;
  var LIST = /^([ \t]*)([-*+]|\d{1,9}[.)])([ \t]+)(.*)$/;
  var FENCE = /^([ \t]{0,3})(`{3,}|~{3,})[ \t]*(.*)$/;

  /**
   * Reads the kind of every line of a document.
   *
   * The lines are read in two passes: the first one finds the blocks that
   * are opened by a single line, the second one finds the tables, which
   * need a header and a separator.
   *
   * @param {Array} lines the lines of the document
   * @returns {Array} one record per line
   */
  function analyse(lines) {
    var records = [];
    var index = 0;
    var fence = null;

    while (index < lines.length) {
      var line = lines[index];
      var body = withoutCarriageReturn(line.text);
      var record = {
        text: line.text,
        body: body,
        carriageReturn: body.length === line.text.length ? '' : '\r',
        terminator: line.terminator,
        offset: line.offset,
        kind: 'text'
      };

      var opened = FENCE.exec(body);
      if (fence !== null) {
        var closes = opened !== null
          && opened[2].charAt(0) === fence.marker.charAt(0)
          && opened[2].length >= fence.marker.length
          && opened[3].trim() === '';
        record.kind = closes ? 'fence-close' : 'code';
        if (closes) {
          fence = null;
        }
        records.push(record);
        index += 1;
        continue;
      }

      if (opened !== null) {
        record.kind = 'fence-open';
        record.marker = opened[2];
        record.info = opened[3].trim();
        record.language = languageOf(record.info);
        fence = { marker: opened[2] };
        records.push(record);
        index += 1;
        continue;
      }

      if (/^[ \t]*$/.test(body)) {
        record.kind = 'blank';
        records.push(record);
        index += 1;
        continue;
      }

      if (RULE.test(body)) {
        record.kind = 'rule';
        records.push(record);
        index += 1;
        continue;
      }

      var heading = HEADING.exec(body);
      if (heading !== null) {
        record.kind = 'heading';
        record.level = heading[1].length;
        record.marker = heading[1] + heading[2];
        record.content = heading[3];
        record.renderContent = headingText(heading[3]);
        records.push(record);
        index += 1;
        continue;
      }

      var quote = QUOTE.exec(body);
      if (quote !== null) {
        record.kind = 'quote';
        record.marker = quote[1];
        record.content = quote[2];
        records.push(record);
        index += 1;
        continue;
      }

      var item = LIST.exec(body);
      if (item !== null) {
        record.kind = 'list';
        record.indentText = item[1];
        record.indent = item[1].length;
        record.ordered = /[0-9]/.test(item[2]);
        record.marker = item[2];
        record.gap = item[3];
        record.content = item[4];
        records.push(record);
        index += 1;
        continue;
      }

      records.push(record);
      index += 1;
    }

    markTables(records);
    return records;
  }

  /**
   * Finds the language of a fenced block.
   *
   * @param {string} info the text that follows the fence
   * @returns {string} the language, or an empty string when there is none
   */
  function languageOf(info) {
    var trimmed = info.trim();
    if (trimmed === '') {
      return '';
    }
    var space = trimmed.search(/\s/);
    return space < 0 ? trimmed : trimmed.substring(0, space);
  }

  /**
   * Takes the closing hashes off the text of a heading.
   *
   * @param {string} content the text of the heading
   * @returns {string} the text of the heading
   */
  function headingText(content) {
    var trimmed = content.replace(/[ \t]+$/, '');
    var closing = /^(.*?)[ \t]+#+[ \t]*$/.exec(trimmed);
    if (closing !== null && closing[1].trim() !== '') {
      return closing[1];
    }
    return trimmed;
  }

  /**
   * Finds the tables of a document.
   *
   * A table is a header row, a row of separators and the rows that follow.
   * The separator row needs a pipe in it, which is what tells it apart from
   * a horizontal rule.
   *
   * @param {Array} records the records of the document
   * @returns {void}
   */
  function markTables(records) {
    var index = 0;
    while (index < records.length) {
      if (records[index].kind !== 'text' || records[index].body.indexOf('|') < 0) {
        index += 1;
        continue;
      }
      var next = records[index + 1];
      if (next === undefined || next.kind !== 'text') {
        index += 1;
        continue;
      }
      var aligns = separatorRow(next.body);
      if (aligns === null) {
        index += 1;
        continue;
      }
      records[index].kind = 'table-head';
      records[index].aligns = aligns;
      next.kind = 'table-sep';
      next.aligns = aligns;
      var row = index + 2;
      while (row < records.length
        && records[row].kind === 'text'
        && records[row].body.indexOf('|') >= 0) {
        records[row].kind = 'table-row';
        records[row].aligns = aligns;
        row += 1;
      }
      index = row;
    }
  }

  /**
   * Reads the alignments of the separator row of a table.
   *
   * @param {string} body the text of the row
   * @returns {Array} the alignment of every cell, or null when the row is
   *                 not a separator row
   */
  function separatorRow(body) {
    if (body.indexOf('|') < 0) {
      return null;
    }
    var cells = splitCells(body);
    if (cells.length === 0) {
      return null;
    }
    var aligns = [];
    for (var index = 0; index < cells.length; index += 1) {
      if (!/^:?-+:?$/.test(cells[index])) {
        return null;
      }
      var cell = cells[index];
      var left = cell.charAt(0) === ':';
      var right = cell.charAt(cell.length - 1) === ':';
      if (left && right) {
        aligns.push('center');
      } else if (right) {
        aligns.push('right');
      } else if (left) {
        aligns.push('left');
      } else {
        aligns.push('');
      }
    }
    return aligns;
  }

  /**
   * Cuts a row of a table into its cells.
   *
   * @param {string} body the text of the row
   * @returns {Array} the text of every cell
   */
  function splitCells(body) {
    var cells = [];
    var current = '';
    var index = 0;
    while (index < body.length) {
      var character = body.charAt(index);
      if (character === '\\' && index + 1 < body.length) {
        current += character + body.charAt(index + 1);
        index += 2;
        continue;
      }
      if (character === '|') {
        cells.push(current);
        current = '';
        index += 1;
        continue;
      }
      current += character;
      index += 1;
    }
    cells.push(current);
    if (cells.length > 1 && cells[0].trim() === '') {
      cells.shift();
    }
    if (cells.length > 1 && cells[cells.length - 1].trim() === '') {
      cells.pop();
    }
    for (var position = 0; position < cells.length; position += 1) {
      cells[position] = cells[position].trim();
    }
    return cells;
  }

  /* ---------- the renderer ---------- */

  /**
   * Renders a Markdown document.
   *
   * @param {string} text the Markdown source
   * @returns {string} the HTML of the document
   */
  function render(text) {
    var source = text === null || text === undefined ? '' : String(text);
    var records = analyse(splitLines(source));
    return renderRange(records, 0, records.length);
  }

  /**
   * Renders the blocks of a range of lines.
   *
   * @param {Array} records the records of the document
   * @param {number} from the first line of the range
   * @param {number} to the line after the range
   * @returns {string} the HTML of the range
   */
  function renderRange(records, from, to) {
    var html = [];
    var index = from;

    while (index < to) {
      var record = records[index];

      if (record.kind === 'blank') {
        index += 1;
        continue;
      }

      if (record.kind === 'fence-open') {
        var code = [];
        var line = index + 1;
        while (line < to && records[line].kind !== 'fence-close') {
          code.push(records[line].body);
          line += 1;
        }
        var attribute = record.language === '' ? '' : ' class="language-' + escapeHtml(record.language) + '"';
        html.push('<pre><code' + attribute + '>' + escapeHtml(code.join('\n')) + '</code></pre>');
        index = line < to ? line + 1 : to;
        continue;
      }

      if (record.kind === 'fence-close' || record.kind === 'code') {
        index += 1;
        continue;
      }

      if (record.kind === 'heading') {
        var level = record.level;
        html.push('<h' + level + '>' + renderInlineText(record.renderContent) + '</h' + level + '>');
        index += 1;
        continue;
      }

      if (record.kind === 'rule') {
        html.push('<hr>');
        index += 1;
        continue;
      }

      if (record.kind === 'quote') {
        var end = index;
        while (end < to && records[end].kind === 'quote') {
          end += 1;
        }
        var inner = [];
        for (var position = index; position < end; position += 1) {
          inner.push({ text: records[position].content, terminator: '\n', offset: 0 });
        }
        var quoted = analyse(inner);
        html.push('<blockquote>' + renderRange(quoted, 0, quoted.length) + '</blockquote>');
        index = end;
        continue;
      }

      if (record.kind === 'list') {
        var region = listEnd(records, index, to);
        html.push(renderList(records, index, region));
        index = region;
        continue;
      }

      if (record.kind === 'table-head') {
        var tableEnd = index;
        while (tableEnd < to && isTableLine(records[tableEnd].kind)) {
          tableEnd += 1;
        }
        html.push(renderTable(records, index, tableEnd));
        index = tableEnd;
        continue;
      }

      var paragraph = [];
      while (index < to && records[index].kind === 'text') {
        paragraph.push(records[index].body);
        index += 1;
      }
      if (paragraph.length === 0) {
        index += 1;
        continue;
      }
      html.push('<p>' + renderInlineLines(paragraph) + '</p>');
    }

    return html.join('');
  }

  /**
   * Tells whether a line belongs to a table.
   *
   * @param {string} kind the kind of the line
   * @returns {boolean} true when the line belongs to a table
   */
  function isTableLine(kind) {
    return kind === 'table-head' || kind === 'table-sep' || kind === 'table-row';
  }

  /**
   * Renders the inline markup of the lines of a paragraph.
   *
   * Two spaces at the end of a line are a hard break, every other line
   * ending is a soft one.
   *
   * @param {Array} lines the lines of the paragraph
   * @returns {string} the HTML of the paragraph
   */
  function renderInlineLines(lines) {
    var html = [];
    for (var index = 0; index < lines.length; index += 1) {
      var line = lines[index];
      var hard = /[ \t]{2,}$/.test(line);
      if (hard) {
        line = line.replace(/[ \t]+$/, '');
      }
      html.push(renderInlineText(line));
      if (index < lines.length - 1) {
        html.push(hard ? '<br />' : '\n');
      }
    }
    return html.join('');
  }

  /**
   * Renders the inline markup of one line.
   *
   * @param {string} text the line
   * @returns {string} the HTML of the line
   */
  function renderInlineText(text) {
    return renderNodes(inlineNodes(text));
  }

  /**
   * Renders a list of inline nodes.
   *
   * @param {Array} nodes the nodes
   * @returns {string} the HTML of the nodes
   */
  function renderNodes(nodes) {
    var html = '';
    for (var index = 0; index < nodes.length; index += 1) {
      var node = nodes[index];
      if (node.type === 'text' || node.type === 'escape') {
        html += escapeHtml(node.text);
      } else if (node.type === 'bold') {
        html += '<strong>' + renderNodes(node.children) + '</strong>';
      } else if (node.type === 'italic') {
        html += '<em>' + renderNodes(node.children) + '</em>';
      } else if (node.type === 'code') {
        html += '<code>' + escapeHtml(node.text) + '</code>';
      } else if (node.type === 'link') {
        html += '<a href="' + escapeHtml(safeUrl(node.url)) + '">' + renderNodes(node.children) + '</a>';
      } else if (node.type === 'image') {
        html += '<img src="' + escapeHtml(safeUrl(node.url)) + '" alt="' + escapeHtml(node.alt) + '">';
      }
    }
    return html;
  }

  /**
   * Finds where a list ends.
   *
   * @param {Array} records the records of the document
   * @param {number} from the first line of the list
   * @param {number} to the line after the range
   * @returns {number} the line after the list
   */
  function listEnd(records, from, to) {
    var ordered = records[from].ordered;
    var index = from;
    var end = from;
    while (index < to) {
      if (records[index].kind === 'list') {
        if (records[index].ordered !== ordered) {
          break;
        }
        end = index + 1;
        index += 1;
        continue;
      }
      if (records[index].kind === 'blank') {
        var next = index;
        while (next < to && records[next].kind === 'blank') {
          next += 1;
        }
        if (next < to && records[next].kind === 'list' && records[next].ordered === ordered) {
          index = next;
          continue;
        }
      }
      break;
    }
    return end;
  }

  /**
   * Renders a list.
   *
   * @param {Array} records the records of the document
   * @param {number} from the first line of the list
   * @param {number} to the line after the list
   * @returns {string} the HTML of the list
   */
  function renderList(records, from, to) {
    var ordered = records[from].ordered;
    var html = ordered ? '<ol>' : '<ul>';
    var index = from;

    while (index < to) {
      if (records[index].kind !== 'list') {
        index += 1;
        continue;
      }
      if (records[index].ordered !== ordered) {
        break;
      }
      var item = { record: records[index], from: index + 1, to: index + 1 };
      var line = index + 1;
      while (line < to) {
        if (records[line].kind === 'list' && records[line].indent <= item.record.indent) {
          break;
        }
        if (records[line].kind === 'blank') {
          var next = line;
          while (next < to && records[next].kind === 'blank') {
            next += 1;
          }
          if (next < to && records[next].kind === 'list' && records[next].indent > item.record.indent) {
            line = next;
            continue;
          }
          break;
        }
        line += 1;
      }
      item.to = line;
      html += renderItem(records, item);
      index = line;
    }

    return html + (ordered ? '</ol>' : '</ul>');
  }

  /**
   * Renders one item of a list.
   *
   * The lines that follow the marker belong to the item: a list deeper down
   * becomes a list inside the item, every other line continues its text.
   *
   * @param {Array} records the records of the document
   * @param {Object} item the item, with the range of its lines
   * @returns {string} the HTML of the item
   */
  function renderItem(records, item) {
    var text = item.record.content;
    var nested = [];
    var index = item.from;

    while (index < item.to) {
      if (records[index].kind === 'list') {
        var end = index;
        while (end < item.to && records[end].kind === 'list') {
          end += 1;
        }
        nested.push(renderList(records, index, end));
        index = end;
        continue;
      }
      if (records[index].kind === 'blank') {
        index += 1;
        continue;
      }
      text += '\n' + records[index].body.replace(/^[ \t]+/, '');
      index += 1;
    }

    return '<li>' + renderInlineLines(text.split('\n')) + nested.join('') + '</li>';
  }

  /**
   * Renders a table.
   *
   * @param {Array} records the records of the document
   * @param {number} from the line of the header
   * @param {number} to the line after the table
   * @returns {string} the HTML of the table
   */
  function renderTable(records, from, to) {
    var aligns = records[from].aligns || [];
    var header = splitCells(records[from].body);
    var html = '<table><thead><tr>';
    for (var index = 0; index < header.length; index += 1) {
      html += '<th' + alignment(aligns[index]) + '>' + renderInlineText(header[index]) + '</th>';
    }
    html += '</tr></thead><tbody>';
    for (var row = from + 2; row < to; row += 1) {
      var cells = splitCells(records[row].body);
      html += '<tr>';
      for (var column = 0; column < header.length; column += 1) {
        html += '<td' + alignment(aligns[column]) + '>' + renderInlineText(column < cells.length ? cells[column] : '') + '</td>';
      }
      html += '</tr>';
    }
    return html + '</tbody></table>';
  }

  /**
   * Reads the style of a cell.
   *
   * @param {string} align the alignment of the column
   * @returns {string} the style of the cell, or an empty string
   */
  function alignment(align) {
    if (align === 'left' || align === 'right' || align === 'center') {
      return ' style="text-align:' + align + '"';
    }
    return '';
  }

  /* ---------- the highlighter ---------- */

  /**
   * Marks up the source of a document.
   *
   * The output is the source itself, with the Markdown syntax wrapped in
   * spans, and with the matched range wrapped in a span of its own.  Once
   * the markup is taken away, what is left is the source, character for
   * character.
   *
   * @param {string} text the Markdown source
   * @param {number} start where the match starts
   * @param {number} end where the match ends
   * @returns {string} the HTML of the source
   */
  function highlight(text, start, end) {
    var source = text === null || text === undefined ? '' : String(text);
    var records = analyse(splitLines(source));

    var from = -1;
    var to = -1;
    if (typeof start === 'number' && typeof end === 'number' && isFinite(start) && isFinite(end)) {
      from = Math.max(0, Math.floor(start));
      to = Math.min(source.length, Math.floor(end));
    }

    var html = '';
    for (var index = 0; index < records.length; index += 1) {
      var record = records[index];
      var pieces = linePieces(record);
      if (record.carriageReturn !== '') {
        pieces.push({ text: record.carriageReturn, classes: [] });
      }
      var segments = [{ pieces: pieces, hit: false }];
      if (from >= 0 && to > from) {
        var localFrom = Math.max(0, from - record.offset);
        var localTo = Math.min(lengthOf(pieces), to - record.offset);
        if (localTo > localFrom) {
          segments = markHit(pieces, localFrom, localTo);
        }
      }
      html += emitSegments(segments);
      html += record.terminator;
    }
    return html;
  }

  /**
   * Reads the length of the text of a list of pieces.
   *
   * @param {Array} pieces the pieces
   * @returns {number} the number of characters
   */
  function lengthOf(pieces) {
    var length = 0;
    for (var index = 0; index < pieces.length; index += 1) {
      length += pieces[index].text.length;
    }
    return length;
  }

  /**
   * Marks the pieces that fall inside the matched range.
   *
   * @param {Array} pieces the pieces of a line
   * @param {number} from where the range starts in the line
   * @param {number} to where the range ends in the line
   * @returns {Array} the segments, each with the pieces it holds
   */
  function markHit(pieces, from, to) {
    var items = [];
    var position = 0;
    for (var index = 0; index < pieces.length; index += 1) {
      var piece = pieces[index];
      var pieceStart = position;
      var pieceEnd = position + piece.text.length;
      position = pieceEnd;
      var start = Math.max(from, pieceStart);
      var end = Math.min(to, pieceEnd);
      if (start >= end) {
        items.push({ text: piece.text, classes: piece.classes, hit: false });
        continue;
      }
      if (start > pieceStart) {
        items.push({ text: piece.text.substring(0, start - pieceStart), classes: piece.classes, hit: false });
      }
      items.push({ text: piece.text.substring(start - pieceStart, end - pieceStart), classes: piece.classes, hit: true });
      if (end < pieceEnd) {
        items.push({ text: piece.text.substring(end - pieceStart), classes: piece.classes, hit: false });
      }
    }

    var segments = [];
    var current = null;
    for (var at = 0; at < items.length; at += 1) {
      var item = items[at];
      if (item.hit) {
        if (current === null) {
          current = { pieces: [], hit: true };
          segments.push(current);
        }
        current.pieces.push({ text: item.text, classes: item.classes });
        continue;
      }
      current = null;
      segments.push({ pieces: [{ text: item.text, classes: item.classes }], hit: false });
    }
    return segments;
  }

  /**
   * Writes the pieces out as HTML.
   *
   * @param {Array} segments the segments, each with the pieces it holds
   * @returns {string} the HTML of the segments
   */
  function emitSegments(segments) {
    var html = '';
    for (var index = 0; index < segments.length; index += 1) {
      var segment = segments[index];
      var inner = '';
      for (var position = 0; position < segment.pieces.length; position += 1) {
        var piece = segment.pieces[position];
        var text = escapeHtml(piece.text);
        if (piece.classes.length > 0) {
          text = '<span class="' + piece.classes.join(' ') + '">' + text + '</span>';
        }
        inner += text;
      }
      if (segment.hit) {
        inner = '<span class="tok-hit">' + inner + '</span>';
      }
      html += inner;
    }
    return html;
  }

  /**
   * Reads the pieces of one line.
   *
   * The pieces hold the text of the line, in order, so that they add up to
   * the line itself.
   *
   * @param {Object} record the record of the line
   * @returns {Array} the pieces of the line
   */
  function linePieces(record) {
    var pieces = [];
    var body = record.body;

    if (record.kind === 'code') {
      pieces.push({ text: body, classes: ['tok-code'] });
      return pieces;
    }

    if (record.kind === 'fence-open' || record.kind === 'fence-close') {
      pieces.push({ text: body, classes: [] });
      return pieces;
    }

    if (record.kind === 'heading') {
      pieces.push({ text: record.marker, classes: [] });
      inlinePieces(inlineNodes(record.content), ['tok-head'], pieces);
      return pieces;
    }

    if (record.kind === 'rule') {
      pieces.push({ text: body, classes: ['tok-rule'] });
      return pieces;
    }

    if (record.kind === 'quote') {
      pieces.push({ text: record.marker, classes: [] });
      inlinePieces(inlineNodes(record.content), ['tok-quote'], pieces);
      return pieces;
    }

    if (record.kind === 'list') {
      pieces.push({ text: record.indentText, classes: [] });
      pieces.push({ text: record.marker, classes: ['tok-list'] });
      pieces.push({ text: record.gap, classes: [] });
      inlinePieces(inlineNodes(record.content), [], pieces);
      return pieces;
    }

    if (record.kind === 'table-head' || record.kind === 'table-row') {
      tablePieces(body, pieces);
      return pieces;
    }

    if (record.kind === 'table-sep') {
      pieces.push({ text: body, classes: ['tok-table'] });
      return pieces;
    }

    inlinePieces(inlineNodes(body), [], pieces);
    return pieces;
  }

  /**
   * Reads the pieces of a row of a table.
   *
   * The pipes are marked, the cells are read as inline text.
   *
   * @param {string} body the text of the row
   * @param {Array} pieces the pieces to fill
   * @returns {void}
   */
  function tablePieces(body, pieces) {
    var start = 0;
    var index = 0;
    while (index < body.length) {
      var character = body.charAt(index);
      if (character === '\\' && index + 1 < body.length) {
        index += 2;
        continue;
      }
      if (character === '|') {
        if (index > start) {
          inlinePieces(inlineNodes(body.substring(start, index)), [], pieces);
        }
        pieces.push({ text: '|', classes: ['tok-table'] });
        start = index + 1;
      }
      index += 1;
    }
    if (start < body.length) {
      inlinePieces(inlineNodes(body.substring(start)), [], pieces);
    }
  }

  /**
   * Reads the pieces of the inline markup of a line.
   *
   * @param {Array} nodes the nodes of the line
   * @param {Array} classes the classes the pieces carry
   * @param {Array} pieces the pieces to fill
   * @returns {void}
   */
  function inlinePieces(nodes, classes, pieces) {
    for (var index = 0; index < nodes.length; index += 1) {
      var node = nodes[index];
      if (node.type === 'text') {
        pieces.push({ text: node.text, classes: classes });
      } else if (node.type === 'escape') {
        pieces.push({ text: node.raw, classes: classes });
      } else if (node.type === 'bold') {
        pieces.push({ text: node.mark, classes: classes });
        inlinePieces(node.children, classes.concat(['tok-bold']), pieces);
        pieces.push({ text: node.mark, classes: classes });
      } else if (node.type === 'italic') {
        pieces.push({ text: node.mark, classes: classes });
        inlinePieces(node.children, classes.concat(['tok-italic']), pieces);
        pieces.push({ text: node.mark, classes: classes });
      } else if (node.type === 'code') {
        pieces.push({ text: node.mark, classes: classes });
        pieces.push({ text: node.raw, classes: classes.concat(['tok-code']) });
        pieces.push({ text: node.mark, classes: classes });
      } else if (node.type === 'link') {
        pieces.push({ text: node.mark, classes: classes });
        inlinePieces(node.children, classes.concat(['tok-link']), pieces);
        pieces.push({ text: node.close, classes: classes });
        pieces.push({ text: node.url, classes: classes.concat(['tok-url']) });
        pieces.push({ text: ')', classes: classes });
      } else if (node.type === 'image') {
        pieces.push({ text: node.mark, classes: classes });
        pieces.push({ text: node.alt, classes: classes });
        pieces.push({ text: node.close, classes: classes });
        pieces.push({ text: node.url, classes: classes.concat(['tok-url']) });
        pieces.push({ text: ')', classes: classes });
      }
    }
  }

  /* ---------- the page ---------- */

  window.RegieMarkdown = {
    render: render,
    highlight: highlight
  };
}());
