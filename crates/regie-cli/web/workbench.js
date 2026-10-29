// The workbench: the page, the engine and the editor together.
//
// The page has two modes.  In the intent mode a request in plain language is
// interpreted into a specification, and the engine compiles that.  In the
// pattern mode the pattern goes straight to the compiler, which is the path an
// advanced user takes.  The document is the haystack in both.

(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);

  const dom = {
    query: $("query"),
    queryLabel: $("query-label"),
    queryInfo: $("query-info"),
    flags: $("flags"),
    flagI: $("flag-i"),
    flagM: $("flag-m"),
    flagS: $("flag-s"),
    document: $("document"),
    preview: $("preview"),
    previewInfo: $("preview-info"),
    gutter: $("gutter"),
    highlight: $("highlight"),
    band: $("band"),
    strip: $("strip"),
    console: $("console"),
    consoleInfo: $("console-info"),
    documentInfo: $("document-info"),
    dot: $("dot"),
    dot2: $("dot2"),
    sDot: $("s-dot"),
    verdict: $("verdict"),
    verdict2: $("verdict2"),
    sVerdict: $("s-verdict"),
    kKind: $("k-kind"),
    kSpec: $("k-spec"),
    kReason: $("k-reason"),
    kNote: $("k-note"),
    mNote: $("m-note"),
    params: $("params"),
    out: $("out"),
    groups: $("groups"),
    groupsEmpty: $("groups-empty"),
    auto: $("auto"),
    sSpec: $("s-spec"),
    sCaret: $("s-caret"),
    sLang: $("s-lang"),
    sEngine: $("s-engine"),
    examples: $("examples"),
    exampleCount: $("example-count"),
    families: $("families"),
    dialect: $("dialect"),
    docInfo: $("doc-info"),
    crumb: $("crumb"),
  };

  // The document the workbench opens with.  It has a table in it, because a
  // table is where a regex earns its keep.
  const DOCUMENT = [
    "# The quarterly report",
    "",
    "The intent layer turns a request in plain language into a specification,",
    "and the compiler turns that specification into an automaton.  The model",
    "never writes a pattern: it chooses one value out of a closed enumeration.",
    "",
    "## The families",
    "",
    "| family | kind | anchored | notes |",
    "|---|---|---|---|",
    "| brazilian_cpf | validation | yes | 11 digits and two check digits |",
    "| brazilian_cnpj | validation | yes | 14 digits and two check digits |",
    "| portuguese_nif | validation | yes | 9 digits and one check digit |",
    "| emails | extraction | no | the value inside a sentence |",
    "| numbers | extraction | no | the digits of the text |",
    "| between_delimiters | extraction | no | what sits between two marks |",
    "",
    "## The guarantees",
    "",
    "- no catastrophic backtracking",
    "- the worst case is O(m*n)",
    "- the model refuses to guess",
    "",
    "Write to ana@corp.com about the order, or to the desk on 123.456.789-09.",
    "",
  ].join("\n");

  // The examples.  Each one is a request or a pattern and a document.
  const EXAMPLES = [
    {
      mode: "intent",
      query: "extract the email",
      note: "emails",
      family: "an extraction, not anchored",
    },
    {
      mode: "intent",
      query: "validate a CPF",
      note: "brazilian_cpf",
      family: "a validation, anchored",
    },
    {
      mode: "intent",
      query: "extract the numbers between brackets",
      note: "delimiters",
      family: "the value between two marks",
    },
    {
      mode: "pattern",
      query: "^\\| [a-z_]+ \\|",
      flags: "m",
      note: "table row",
      family: "the first column of a table row, line by line",
    },
    {
      mode: "pattern",
      query: "\\| (?:yes|no) \\|",
      note: "cell value",
      family: "the cells of the third column",
    },
    {
      mode: "pattern",
      query: "(?i)cpf",
      note: "flags",
      family: "the case-insensitive flag, inside the pattern",
    },
    {
      mode: "pattern",
      query: "(?<name>a)",
      note: "refused",
      family: "a construct the engine refuses",
    },
  ];

  // The enumeration the model chooses from.
  const FAMILIES = [
    ["validate", "email"], ["validate", "url"], ["validate", "uri"],
    ["validate", "domain name"], ["validate", "ipv4"], ["validate", "ipv6"],
    ["validate", "mac address"], ["validate", "brazilian cpf"],
    ["validate", "brazilian cnpj"], ["validate", "brazilian postal code"],
    ["validate", "brazilian phone"], ["validate", "brazilian license plate"],
    ["validate", "portuguese nif"], ["validate", "us ssn"], ["validate", "semver"],
    ["validate", "iso date"], ["validate", "iso date time"], ["validate", "clock time"],
    ["validate", "uuid"], ["validate", "hex"], ["validate", "base64"],
    ["validate", "base32"], ["validate", "slug"], ["validate", "username"],
    ["validate", "credit card"], ["validate", "iban"], ["validate", "us zip code"],
    ["validate", "integer"], ["validate", "decimal"], ["validate", "currency amount"],
    ["validate", "json"],
    ["extract", "between delimiters"], ["extract", "quoted"], ["extract", "numbers"],
    ["extract", "words"], ["extract", "urls"], ["extract", "emails"],
    ["extract", "ip addresses"], ["extract", "dates"], ["extract", "after keyword"],
    ["extract", "before keyword"],
    ["filter", "block a term"], ["custom", "a pattern of your own"],
  ];

  // What the dialect accepts, and what it refuses.
  const DIALECT = {
    accepted: [
      ["literal", "any byte that is not a metacharacter"],
      [".", "any byte except a newline"],
      ["^ $", "the start and the end of the text"],
      ["( )", "a capturing group"],
      ["(?: )", "a group that does not capture"],
      ["(?flags )", "a group with its own flags"],
      ["|", "an alternation"],
      ["* + ?", "the greedy quantifiers"],
      ["{n,m}", "a counted repetition, at most 255"],
      ["*? +? ??", "the lazy quantifiers"],
      ["[ ]", "a character class"],
      ["[a-z]", "a range of bytes"],
      ["[^a-z]", "the bytes outside a range"],
      ["\\d \\w \\s", "the shorthands, and their negations"],
      ["\\xHH", "the byte with that value"],
      ["\\b \\B", "a word boundary"],
      ["i m s", "the flags: case-insensitive, multi-line, dot-all"],
    ],
    refused: [
      ["(?= ) (?! )", "look-ahead", "Unsupported"],
      ["(?<= ) (?<! )", "look-behind", "Unsupported"],
      ["\\1", "a backreference", "Unsupported"],
      ["(?<name> )", "a named group", "Unsupported"],
      ["a*+", "a possessive quantifier", "Unsupported"],
      ["\\q", "an unknown escape", "Syntax"],
      ["[^]]", "an unescaped ] in a class", "Syntax"],
      ["[z-a]", "a range that ends before it starts", "Syntax"],
      ["[]", "an empty class", "Syntax"],
      ["nine groups", "more than eight capture groups", "TooManyGroups"],
      ["a{300}", "a repetition bound above 255", "RepetitionTooLarge"],
    ],
  };

  let mode = "intent";
  let lastAnswer = null;
  let analyses = 0;
  let timer = null;
  // Every analysis carries a number.  A late answer that is not the newest
  // one is dropped: the page must never show the answer to a question that
  // was asked before the one on screen.
  let analysisId = 0;
  let automatonId = 0;

  // ---- the editor ----------------------------------------------------------------

  const editor = window.RegieEditor.create({
    textarea: dom.document,
    gutter: dom.gutter,
    highlight: dom.highlight,
    band: dom.band,
    strip: dom.strip,
    onChange: (text) => {
      renderPreview(text);
      schedule();
    },
    onCaret: (line, column) => {
      dom.sCaret.textContent = "Ln " + line + ", Col " + column;
    },
  });

  editor.setText(DOCUMENT);

  function renderPreview(text) {
    dom.preview.innerHTML = window.RegieMarkdown.render(text);
    const lines = text.split("\n").length;
    const words = text.split(/\s+/).filter(Boolean).length;
    dom.previewInfo.textContent = lines + " lines, " + words + " words";
    dom.documentInfo.textContent = lines + " lines, " + text.length + " bytes";
    renderDocumentInfo(text, lines, words);
  }

  // The facts about the document, in the sidebar.
  function renderDocumentInfo(text, lines, words) {
    dom.docInfo.innerHTML = "";
    const table = document.createElement("table");
    const rows = [
      ["lines", String(lines)],
      ["words", String(words)],
      ["bytes", String(new TextEncoder().encode(text).length)],
      ["tables", String((dom.preview.querySelectorAll("table").length))],
      ["headings", String(dom.preview.querySelectorAll("h1, h2, h3, h4, h5, h6").length)],
    ];
    for (const row of rows) {
      const name = document.createElement("td");
      name.className = "k";
      name.textContent = row[0];
      const value = document.createElement("td");
      value.className = "v";
      value.textContent = row[1];
      const line = document.createElement("tr");
      line.append(name, value);
      table.appendChild(line);
    }
    dom.docInfo.appendChild(table);
  }

  // ---- the sidebar ----------------------------------------------------------------

  function buildExamples() {
    dom.exampleCount.textContent = String(EXAMPLES.length);
    EXAMPLES.forEach((example, index) => {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      const name = document.createElement("span");
      name.className = "name";
      name.textContent = example.query;
      const note = document.createElement("span");
      note.className = "tag";
      note.textContent = example.note;
      button.append(name, note);
      button.title = example.family;
      button.addEventListener("click", () => {
        // The query and the flags go in first: the analysis that follows
        // reads them, and a second one would race the first.
        dom.query.value = example.query;
        applyFlags(example.flags || "");
        setMode(example.mode);
        selectExample(index);
      });
      item.appendChild(button);
      dom.examples.appendChild(item);
    });
  }

  function selectExample(index) {
    const buttons = dom.examples.querySelectorAll("button");
    buttons.forEach((button, position) => {
      button.classList.toggle("on", position === index);
    });
  }

  function buildFamilies() {
    FAMILIES.forEach((family) => {
      const item = document.createElement("li");
      const row = document.createElement("div");
      row.className = "node";
      const glyph = document.createElement("span");
      glyph.className = "glyph";
      glyph.textContent = family[0] === "validate" ? "◆" : family[0] === "extract" ? "◇" : "·";
      const name = document.createElement("span");
      name.textContent = family[1];
      const state = document.createElement("span");
      state.className = "state";
      state.textContent = family[0];
      row.append(glyph, name, state);
      item.appendChild(row);
      dom.families.appendChild(item);
    });
  }

  function buildDialect() {
    const accepted = document.createElement("h3");
    accepted.textContent = "accepted";
    const acceptedList = document.createElement("ul");
    DIALECT.accepted.forEach((row) => {
      const item = document.createElement("li");
      const mark = document.createElement("span");
      mark.className = "m";
      mark.textContent = "✓";
      const code = document.createElement("code");
      code.textContent = row[0];
      const note = document.createElement("span");
      note.textContent = row[1];
      item.append(mark, code, note);
      acceptedList.appendChild(item);
    });

    const refused = document.createElement("h3");
    refused.textContent = "refused";
    const refusedList = document.createElement("ul");
    DIALECT.refused.forEach((row) => {
      const item = document.createElement("li");
      const mark = document.createElement("span");
      mark.className = "x";
      mark.textContent = "×";
      const code = document.createElement("code");
      code.textContent = row[0];
      const note = document.createElement("span");
      note.textContent = row[1] + " · " + row[2];
      item.append(mark, code, note);
      refusedList.appendChild(item);
    });

    dom.dialect.append(accepted, acceptedList, refused, refusedList);
  }

  buildExamples();
  buildFamilies();
  buildDialect();

  // ---- the modes ------------------------------------------------------------------

  function setMode(next) {
    mode = next;
    for (const button of document.querySelectorAll(".modes button")) {
      button.classList.toggle("on", button.dataset.mode === mode);
    }
    dom.queryLabel.textContent = mode === "intent" ? "Request" : "Pattern";
    dom.flags.style.display = mode === "pattern" ? "flex" : "none";
    dom.crumb.textContent = mode === "intent" ? "request" : "pattern";
    analyse();
  }

  // Puts the flags of the engine in the checkboxes.
  function applyFlags(text) {
    dom.flagI.checked = text.indexOf("i") >= 0;
    dom.flagM.checked = text.indexOf("m") >= 0;
    dom.flagS.checked = text.indexOf("s") >= 0;
  }

  for (const button of document.querySelectorAll(".modes button")) {
    button.addEventListener("click", () => setMode(button.dataset.mode));
  }

  for (const button of document.querySelectorAll(".rail button")) {
    button.addEventListener("click", () => {
      for (const other of document.querySelectorAll(".rail button")) {
        other.classList.toggle("on", other === button);
      }
      for (const section of document.querySelectorAll(".side section")) {
        section.classList.toggle("on", section.id === "side-" + button.dataset.side);
      }
    });
  }

  for (const button of document.querySelectorAll(".tabs2 button")) {
    button.addEventListener("click", () => {
      for (const other of document.querySelectorAll(".tabs2 button")) {
        other.classList.toggle("on", other === button);
      }
      for (const view of document.querySelectorAll(".pane-view")) {
        view.classList.toggle("on", view.id === "v-" + button.dataset.tab);
      }
      if (button.dataset.tab === "automaton") {
        loadAutomaton();
      }
    });
  }

  // ---- the analysis ---------------------------------------------------------------

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(analyse, 140);
  }

  function flags() {
    let out = "";
    if (dom.flagI.checked) out += "i";
    if (dom.flagM.checked) out += "m";
    if (dom.flagS.checked) out += "s";
    return out;
  }

  function analyse() {
    const query = dom.query.value;
    const text = editor.getText();
    const path = mode === "intent"
      ? "/analyse?request=" + encodeURIComponent(query) +
        "&haystack=" + encodeURIComponent(text)
      : "/pattern?pattern=" + encodeURIComponent(query) +
        "&haystack=" + encodeURIComponent(text) +
        "&flags=" + encodeURIComponent(flags());

    const id = ++analysisId;
    fetch(path)
      .then((response) => response.json())
      .then((answer) => {
        if (id !== analysisId) {
          return;
        }
        lastAnswer = answer;
        render(answer);
        loadAutomaton();
      })
      .catch(() => {
        if (id !== analysisId) {
          return;
        }
        dom.verdict.textContent = "the server did not answer";
      });
  }

  function render(answer) {
    dom.groups.innerHTML = "";
    const text = editor.getText();

    if (answer.status === "error") {
      dom.dot.className = "dot err";
      dom.dot2.className = "dot err";
      dom.sDot.className = "dot err";
      dom.verdict.textContent = "the engine refused the pattern";
      dom.verdict2.textContent = dom.verdict.textContent;
      dom.sVerdict.textContent = answer.error_kind || "error";
      dom.kKind.textContent = "error";
      dom.kSpec.textContent = answer.error || "-";
      dom.kReason.textContent = answer.error_position === undefined
        ? "-"
        : "at byte " + answer.error_position;
      dom.kNote.className = "note err";
      dom.kNote.textContent = answer.error || "";
      dom.mNote.className = "note err";
      dom.mNote.textContent = "";
      dom.out.textContent = text;
      dom.groupsEmpty.textContent = "no groups";
      dom.params.innerHTML = "";
      editor.setMatch(null, null);
      setPath("error");
      log(answer);
      return;
    }

    if (answer.status === "ambiguous") {
      dom.dot.className = "dot amb";
      dom.dot2.className = "dot amb";
      dom.sDot.className = "dot amb";
      dom.verdict.textContent = "the model did not settle on one specification";
      dom.verdict2.textContent = dom.verdict.textContent;
      dom.sVerdict.textContent = "ambiguous";
      dom.kKind.textContent = "ambiguous";
      dom.kSpec.textContent = (answer.suggestions || []).join(", ") || "-";
      dom.kReason.textContent = answer.reason || "-";
      dom.kNote.className = "note warn";
      dom.kNote.textContent =
        "The model refused to guess. The candidates above are the families it weighed, and the caller has to choose one.";
      dom.mNote.className = "note warn";
      dom.mNote.textContent = "";
      dom.out.textContent = text;
      dom.groupsEmpty.textContent = "no groups";
      dom.params.innerHTML = "";
      editor.setMatch(null, null);
      setPath("ambiguous");
      log(answer);
      return;
    }

    const isPattern = answer.status === "compiled";
    const spec = answer.spec || "";
    if (isPattern) {
      dom.kKind.textContent = answer.pattern || "-";
      dom.kSpec.textContent = answer.flags ? "flags " + answer.flags : "no flags";
      dom.kReason.textContent = answer.matched ? "the pattern matches" : "the pattern compiles";
      dom.kNote.className = "note";
      dom.kNote.textContent =
        "A pattern goes straight to the compiler. The model is not involved: this is the path an advanced user takes.";
      dom.params.innerHTML = "";
    } else {
      dom.kKind.textContent = answer.kind || spec.split(" ")[0] || "-";
      dom.kSpec.textContent = spec || "-";
      dom.kReason.textContent = answer.anchored ? "anchored" : "not anchored";
      dom.kNote.className = "note";
      dom.kNote.textContent = answer.anchored
        ? "A validation is anchored: the whole haystack has to be the value. A validation never looks for the value inside a sentence."
        : "An extraction is not anchored: it finds the value inside the sentence and reports the capture groups.";
      buildParams(spec);
    }
    dom.sSpec.textContent = dom.kKind.textContent;

    if (answer.matched) {
      dom.dot.className = "dot ok";
      dom.dot2.className = "dot ok";
      dom.sDot.className = "dot ok";
      dom.verdict.textContent = "matched " + answer.start + ".." + answer.end;
      dom.verdict2.textContent = dom.verdict.textContent;
      dom.sVerdict.textContent = dom.verdict.textContent;
      mark(text, answer.start, answer.end);
      dom.mNote.className = "note";
      dom.mNote.textContent = "The match is reported in byte offsets, the way the engine counts.";
      editor.setMatch(answer.start, answer.end);
    } else {
      dom.dot.className = "dot";
      dom.dot2.className = "dot";
      dom.sDot.className = "dot";
      dom.verdict.textContent = "no match";
      dom.verdict2.textContent = "no match";
      dom.sVerdict.textContent = "no match";
      dom.out.textContent = text;
      dom.mNote.className = "note";
      dom.mNote.textContent = answer.anchored
        ? "A validation is anchored: the whole haystack has to be the value."
        : "";
      editor.setMatch(null, null);
    }

    if (answer.groups && answer.groups.length) {
      dom.groupsEmpty.style.display = "none";
      for (const group of answer.groups) {
        const row = document.createElement("tr");
        const number = document.createElement("td");
        number.className = "n";
        number.textContent = group.index;
        const span = document.createElement("td");
        span.textContent = group.start + ".." + group.end;
        const value = document.createElement("td");
        value.className = "v";
        value.textContent = JSON.stringify(group.text);
        row.append(number, span, value);
        dom.groups.appendChild(row);
      }
    } else {
      dom.groupsEmpty.style.display = "block";
      dom.groupsEmpty.textContent = "no groups";
    }

    dom.queryInfo.textContent = dom.query.value.length + " chars";
    setPath("confident");
    log(answer);
  }

  function buildParams(spec) {
    dom.params.innerHTML = "";
    const pairs = spec.match(/([a-z_]+)=([^\s]+)/g);
    if (!pairs) {
      return;
    }
    const heading = document.createElement("h3");
    heading.textContent = "parameters";
    const table = document.createElement("table");
    for (const pair of pairs) {
      const [key, value] = pair.split("=");
      const row = document.createElement("tr");
      const name = document.createElement("td");
      name.className = "k";
      name.textContent = key;
      const content = document.createElement("td");
      content.className = "v";
      content.textContent = value;
      row.append(name, content);
      table.appendChild(row);
    }
    dom.params.append(heading, table);
  }

  function mark(text, start, end) {
    dom.out.textContent = "";
    const from = charIndexAt(text, start);
    const to = charIndexAt(text, end);
    dom.out.append(text.slice(0, from));
    const element = document.createElement("mark");
    element.textContent = text.slice(from, to);
    dom.out.append(element, text.slice(to));
  }

  function charIndexAt(text, byteOffset) {
    const encoder = new TextEncoder();
    let used = 0;
    let index = 0;
    while (used < byteOffset && index < text.length) {
      const code = text.codePointAt(index);
      const size = code > 0xffff ? 2 : 1;
      used += encoder.encode(String.fromCodePoint(code)).length;
      index += size;
    }
    return index;
  }

  function setPath(status) {
    const on = status === "confident";
    $("p-intent").className = on ? "on" : "idle";
    $("p-search").className = "idle";
    $("p-core").className = on ? "on" : "idle";
    $("p-codec").className = "idle";
    $("n-intent").className = "node on";
    $("n-core").className = "node on";
    $("s-intent").textContent = on ? "on" : "refused";
    $("s-core").textContent = on ? "on" : "idle";
  }

  // ---- the console ----------------------------------------------------------------

  function log(answer) {
    analyses += 1;
    const command = document.createElement("div");
    command.className = "cmd";
    command.textContent = "> " + dom.query.value;

    const result = document.createElement("div");
    result.className = "res";
    if (answer.status === "compiled" || answer.status === "confident") {
      result.classList.add(answer.matched ? "hit" : "miss");
      result.textContent = "  " + (answer.kind || answer.pattern || answer.spec) +
        (answer.matched ? "   matched " + answer.start + ".." + answer.end : "   no match");
    } else {
      result.classList.add("miss");
      result.textContent = "  " + (answer.error_kind || answer.reason || answer.status) +
        (answer.suggestions && answer.suggestions.length
          ? "   [" + answer.suggestions.join(", ") + "]" : "");
    }

    dom.console.append(command, result);
    dom.console.scrollTop = dom.console.scrollHeight;
    dom.consoleInfo.textContent = analyses + (analyses === 1 ? " analysis" : " analyses");
  }

  // ---- the automaton --------------------------------------------------------------

  function loadAutomaton() {
    const query = mode === "intent"
      ? "/automaton?request=" + encodeURIComponent(dom.query.value)
      : "/automaton?pattern=" + encodeURIComponent(dom.query.value);
    const id = ++automatonId;
    fetch(query)
      .then((response) => response.json())
      .then((answer) => {
        if (id !== automatonId) {
          return;
        }
        renderAutomaton(answer);
      })
      .catch(() => {
        if (id !== automatonId) {
          return;
        }
        dom.auto.textContent = "the server did not answer";
      });
  }

  function renderAutomaton(answer) {
    dom.auto.innerHTML = "";
    if (answer.status !== "ok") {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = answer.error || answer.reason || "no automaton";
      dom.auto.appendChild(empty);
      return;
    }

    const summary = document.createElement("dl");
    summary.className = "kv";
    const rows = [
      ["kind", answer.kind || answer.pattern || "-"],
      ["states", String(answer.states)],
      ["classes", String(answer.classes)],
      ["accept at end", answer.accept_at_end ? "yes" : "no"],
    ];
    for (const row of rows) {
      const term = document.createElement("dt");
      term.textContent = row[0];
      const value = document.createElement("dd");
      value.textContent = row[1];
      summary.append(term, value);
    }
    dom.auto.appendChild(summary);

    const states = document.createElement("div");
    states.className = "states";
    for (let state = 0; state < answer.states; state += 1) {
      const chip = document.createElement("i");
      chip.textContent = String(state);
      if ((answer.accepting || []).indexOf(state) >= 0) {
        chip.className = "on";
      }
      states.appendChild(chip);
    }
    dom.auto.appendChild(states);

    const table = document.createElement("table");
    const head = document.createElement("tr");
    for (const title of ["from", "class", "to"]) {
      const cell = document.createElement("th");
      cell.textContent = title;
      head.appendChild(cell);
    }
    table.appendChild(head);

    const transitions = answer.transitions || [];
    for (const transition of transitions.slice(0, 200)) {
      const row = document.createElement("tr");
      const from = document.createElement("td");
      from.className = "s";
      from.textContent = transition.from;
      const klass = document.createElement("td");
      klass.textContent = transition.class;
      const to = document.createElement("td");
      to.className = "a";
      to.textContent = transition.to;
      row.append(from, klass, to);
      table.appendChild(row);
    }
    dom.auto.appendChild(table);

    if (transitions.length > 200) {
      const note = document.createElement("div");
      note.className = "empty";
      note.textContent = "and " + (transitions.length - 200) + " more";
      dom.auto.appendChild(note);
    }
  }

  // ---- the wiring -----------------------------------------------------------------

  dom.query.addEventListener("input", schedule);
  for (const flag of [dom.flagI, dom.flagM, dom.flagS]) {
    flag.addEventListener("change", analyse);
  }
  dom.query.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      analyse();
    }
  });

  dom.flags.style.display = "none";
  dom.query.value = EXAMPLES[0].query;
  selectExample(0);
  // The editor already scheduled an analysis when it took the document; a
  // second one here would put the same answer in the console twice.
})();
