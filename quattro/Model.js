// Omarchy10k Control Center — Pure Helper Library
// Stateless utilities for TOML parsing, serialization, and daemon protocol.
// All mutable state lives in Panel.qml as reactive QML properties.
.pragma library

function configDir(xdgConfigHome, home) {
    if (xdgConfigHome) return xdgConfigHome + "/omarchy10k";
    return (home || "/tmp") + "/.config/omarchy10k";
}

function configPath(xdgConfigHome, home) {
    return configDir(xdgConfigHome, home) + "/config.toml";
}

function runtimeDir(xdgRuntimeDir) {
    return xdgRuntimeDir || "/tmp";
}

function buildCommand(name, id) {
    var msg = { type: "control", command: name };
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

function buildConfigGet(id) {
    var msg = { type: "config" };
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

// Curated prompt palettes (terminal ricing classics). Each entry maps the
// palette roles the daemon uses for semantic segment fills. Written to
// [theme.custom] with source=hybrid so it layers over the Omarchy theme.
var CURATED_PALETTES = {
    "tokyo-night": { label: "Tokyo Night", accent: "#7aa2f7", red: "#f7768e", green: "#9ece6a", yellow: "#e0af68", blue: "#7aa2f7", magenta: "#bb9af7", cyan: "#7dcfff", orange: "#ff9e64", muted: "#414868", background: "#1a1b26", foreground: "#c0caf5" },
    "catppuccin":  { label: "Catppuccin",  accent: "#89b4fa", red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af", blue: "#89b4fa", magenta: "#cba6f7", cyan: "#94e2d5", orange: "#fab387", muted: "#6c7086", background: "#1e1e2e", foreground: "#cdd6f4" },
    "gruvbox":     { label: "Gruvbox",     accent: "#83a598", red: "#fb4934", green: "#b8bb26", yellow: "#fabd2f", blue: "#83a598", magenta: "#d3869b", cyan: "#8ec07c", orange: "#fe8019", muted: "#928374", background: "#282828", foreground: "#ebdbb2" },
    "nord":        { label: "Nord",        accent: "#88c0d0", red: "#bf616a", green: "#a3be8c", yellow: "#ebcb8b", blue: "#81a1c1", magenta: "#b48ead", cyan: "#8fbcbb", orange: "#d08770", muted: "#4c566a", background: "#2e3440", foreground: "#eceff4" },
    "dracula":     { label: "Dracula",     accent: "#bd93f9", red: "#ff5555", green: "#50fa7b", yellow: "#f1fa8c", blue: "#8be9fd", magenta: "#ff79c6", cyan: "#8be9fd", orange: "#ffb86c", muted: "#6272a4", background: "#282a36", foreground: "#f8f8f2" },
    "rose-pine":   { label: "Rosé Pine",   accent: "#c4a7e7", red: "#eb6f92", green: "#31748f", yellow: "#f6c177", blue: "#9ccfd8", magenta: "#ebbcba", cyan: "#9ccfd8", orange: "#f6c177", muted: "#6e6a86", background: "#191724", foreground: "#e0def4" },
    "everforest":  { label: "Everforest",  accent: "#a7c080", red: "#e67e80", green: "#a7c080", yellow: "#dbbc7f", blue: "#7fbbb3", magenta: "#d699b6", cyan: "#83c092", orange: "#e69875", muted: "#859289", background: "#2d353b", foreground: "#d3c6aa" },
    "kanagawa":    { label: "Kanagawa",    accent: "#7e9cd8", red: "#ff5d62", green: "#98bb6c", yellow: "#ffa066", blue: "#7e9cd8", magenta: "#957fb8", cyan: "#6a9589", orange: "#ffa066", muted: "#727169", background: "#1f1f28", foreground: "#dcd7ba" }
};

function buildConfigSet(patch, id) {
    var msg = { type: "config", command: "set", config: patch };
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

function buildHello(id) {
    var msg = { type: "hello", version: "0.3" };
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

function buildPreview(context, id) {
    var msg = { type: "preview" };
    if (context) {
        for (var k in context) msg[k] = context[k];
    }
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

function stripAnsi(str) {
    return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "")
              .replace(/\x1b\][^\x07]*\x07/g, "")
              .replace(/\x1b\][^\x1b]*\x1b\\/g, "")
              .replace(/[\x01\x02]/g, "");
}

// ── ANSI → Rich Text (Text.StyledText) ─────────────────────────────────────
// Same tokenizer family as stripAnsi, but SGR foreground/background codes
// (3x/9x, 38;5, 38;2, 4x/10x, 48;5, 48;2) are converted into inline
// <span style="..."> markup for Text.StyledText rendering (panel live
// preview, preset gallery cards). All other escape sequences, OSC strings
// and readline delimiters are dropped. HTML entities are escaped first so
// prompt text containing <, >, & renders literally.

// Fallback base-16 table, used ONLY when no palette is supplied.
//
// These are Tokyo Night's values, and for a long time they were the ONLY
// values: every ANSI-indexed color in every gallery card rendered in Tokyo
// Night regardless of the palette actually being previewed, so a Gruvbox
// user was shown a preview that was not of their prompt. Pass a palette to
// `ansiToRich` and the indexed colors resolve against it instead.
var _ANSI_FG = ["#414868", "#f7768e", "#9ece6a", "#e0af68",
                "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6"];
var _ANSI_FG_BRIGHT = ["#565f89", "#ff7a93", "#b9f27c", "#ff9e64",
                       "#7da6ff", "#c7a9ff", "#9dd6ff", "#c0caf5"];

// ANSI index → palette role. Index 0 (black) maps to `muted` rather than
// `background`: a prompt drawing "black" text on its own background would be
// invisible, and muted is what a terminal's dim role actually is.
var _ANSI_ROLES = ["muted", "red", "green", "yellow",
                   "blue", "magenta", "cyan", "foreground"];

/// Resolve base-16 index `i` (0-7) against a palette, falling back to the
/// built-in table when the palette lacks that role.
function _paletteAnsi(palette, i, bright) {
    if (palette) {
        var hex = palette[_ANSI_ROLES[i]];
        if (hex !== undefined && hex !== null && String(hex).length > 0)
            return String(hex);
    }
    return bright ? _ANSI_FG_BRIGHT[i] : _ANSI_FG[i];
}

function escapeHtml(str) {
    return String(str).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// Newlines must become <br/>, because Text.StyledText collapses whitespace --
// newlines included -- exactly like HTML. A two-line prompt (frame presets,
// prompt.newline) therefore rendered as ONE long line with the second line
// appended, so every card and preview row showed "~/app ⑂ main ╰─❯" run
// together and then clipped. It looked like a wrapping bug and was not.
/// Drop leading blank lines from a render.
///
/// `prompt.blank_line` puts an empty line before the prompt so consecutive
/// commands breathe. That is right in a terminal and wrong in a preview: a
/// card is two lines tall, and spending one of them on emptiness cut the
/// "╰─❯" line off every two-line preset.
function stripLeadingBlankLines(text) {
    return String(text === undefined || text === null ? "" : text)
        .replace(/^(?:[ \t]*\r?\n)+/, "");
}

function newlinesToBreaks(str) {
    return String(str).replace(/\r?\n/g, "<br/>");
}

function _hexRgb(r, g, b) {
    function h(v) {
        var s = Math.max(0, Math.min(255, v | 0)).toString(16);
        return s.length < 2 ? "0" + s : s;
    }
    return "#" + h(r) + h(g) + h(b);
}

function _xterm256(idx, palette) {
    if (idx < 16)
        return _paletteAnsi(palette, idx < 8 ? idx : idx - 8, idx >= 8);
    if (idx < 232) {
        var steps = [0, 95, 135, 175, 215, 255];
        var n = idx - 16;
        return _hexRgb(steps[Math.floor(n / 36)], steps[Math.floor((n % 36) / 6)], steps[n % 6]);
    }
    var gray = 8 + 10 * (idx - 232);
    return _hexRgb(gray, gray, gray);
}

function ansiToRich(text, palette) {
    if (text === undefined || text === null) return "";
    text = String(text);

    var out = "";
    var fg = null, bg = null, bold = false, italic = false, underline = false;
    var open = false;

    function closeSpan() {
        if (open) { out += "</span>"; open = false; }
    }

    function openSpan() {
        var styles = "";
        if (fg) styles += "color:" + fg + ";";
        if (bg) styles += "background-color:" + bg + ";";
        if (bold) styles += "font-weight:bold;";
        if (italic) styles += "font-style:italic;";
        if (underline) styles += "text-decoration:underline;";
        if (styles.length === 0) return;
        out += '<span style="' + styles.substring(0, styles.length - 1) + '">';
        open = true;
    }

    function applySgr(params) {
        if (params.length === 0) { fg = bg = null; bold = italic = underline = false; return; }
        var parts = params.split(";");
        var i = 0;
        while (i < parts.length) {
            var n = parseInt(parts[i], 10);
            if (isNaN(n)) n = 0;
            if (n === 0) { fg = bg = null; bold = italic = underline = false; }
            else if (n === 1) bold = true;
            else if (n === 3) italic = true;
            else if (n === 4) underline = true;
            else if (n === 22) bold = false;
            else if (n === 23) italic = false;
            else if (n === 24) underline = false;
            else if (n === 39) fg = null;
            else if (n === 49) bg = null;
            else if (n === 38 || n === 48) {
                var color = null;
                if (parts[i + 1] === "5" && parts[i + 2] !== undefined) {
                    color = _xterm256(parseInt(parts[i + 2], 10) || 0, palette);
                    i += 2;
                } else if (parts[i + 1] === "2" && parts[i + 4] !== undefined) {
                    color = _hexRgb(parseInt(parts[i + 2], 10) || 0,
                                    parseInt(parts[i + 3], 10) || 0,
                                    parseInt(parts[i + 4], 10) || 0);
                    i += 4;
                } else break; // malformed sequence — drop the remainder
                if (n === 38) fg = color; else bg = color;
            }
            else if (n >= 30 && n <= 37) fg = _paletteAnsi(palette, n - 30, false);
            else if (n >= 40 && n <= 47) bg = _paletteAnsi(palette, n - 40, false);
            else if (n >= 90 && n <= 97) fg = _paletteAnsi(palette, n - 90, true);
            else if (n >= 100 && n <= 107) bg = _paletteAnsi(palette, n - 100, true);
            i++;
        }
    }

    var re = /\x1b\[([0-9;?]*)([a-zA-Z])|\x1b\]([^\x07\x1b]*)(?:\x07|\x1b\\)|[\x01\x02]/g;
    var last = 0;
    var m;
    while ((m = re.exec(text)) !== null) {
        if (m.index > last)
            out += newlinesToBreaks(escapeHtml(text.substring(last, m.index)));
        last = re.lastIndex;
        if (m[2] === "m" && m[1].indexOf("?") === -1) {
            var prevFg = fg, prevBg = bg;
            var prevBold = bold, prevItalic = italic, prevUnderline = underline;
            applySgr(m[1]);
            if (fg !== prevFg || bg !== prevBg || bold !== prevBold
                    || italic !== prevItalic || underline !== prevUnderline) {
                closeSpan();
                openSpan();
            }
        }
        // Any other escape sequence (OSC, cursor movement, private modes,
        // readline delimiters) is dropped.
    }
    if (last < text.length)
        out += newlinesToBreaks(escapeHtml(text.substring(last)));
    closeSpan();
    return out;
}

// ── TOML Parser ─────────────────────────────────────────────────────────────
// Handles the subset used by omarchy10k: sections, key = value with
// string, bool, and integer types. Ignores comments and blank lines.

// Strips a comment starting at an unquoted `#`, so `#` inside quoted
// values (e.g. hex colors) survives. Escapes are honored in double quotes.
function stripComment(line) {
    var quote = "";
    for (var i = 0; i < line.length; i++) {
        var ch = line[i];
        if (quote) {
            if (quote === "\"" && ch === "\\") i++;
            else if (ch === quote) quote = "";
        } else if (ch === "\"" || ch === "'") {
            quote = ch;
        } else if (ch === "#") {
            return line.substring(0, i);
        }
    }
    return line;
}

function parseTOML(text) {
    var result = {};
    var section = "";
    var lines = text.split("\n");

    for (var i = 0; i < lines.length; i++) {
        var line = stripComment(lines[i]).trim();
        var secMatch = line.match(/^\[([^\]]+)\]$/);
        if (secMatch) {
            section = secMatch[1];
            continue;
        }

        var eqIdx = line.indexOf("=");
        if (eqIdx < 0) continue;

        var key = line.substring(0, eqIdx).trim();
        var raw = line.substring(eqIdx + 1).trim();
        var fullKey = section ? section + "." + key : key;

        result[fullKey] = parseValue(raw);
    }
    return result;
}

function parseValue(raw) {
    if (raw === "true") return true;
    if (raw === "false") return false;
    if (/^-?\d+$/.test(raw)) return parseInt(raw, 10);
    var strMatch = raw.match(/^"(.*)"$/);
    if (strMatch) return strMatch[1];
    return raw;
}

// ── TOML Builder ────────────────────────────────────────────────────────────
// Serializes a flat dot-keyed object back into sectioned TOML.

function buildTOML(flat) {
    var sections = {};
    var keys = Object.keys(flat);

    for (var i = 0; i < keys.length; i++) {
        var k = keys[i];
        var lastDot = k.lastIndexOf(".");
        var sec = lastDot > 0 ? k.substring(0, lastDot) : "";
        var name = lastDot > 0 ? k.substring(lastDot + 1) : k;

        if (!sections[sec]) sections[sec] = [];
        sections[sec].push({ key: name, value: flat[k] });
    }

    var out = "";
    var sectionNames = Object.keys(sections).sort();
    for (var s = 0; s < sectionNames.length; s++) {
        var sec = sectionNames[s];
        if (sec.length > 0) {
            if (out.length > 0) out += "\n";
            out += "[" + sec + "]\n";
        }
        var entries = sections[sec];
        for (var e = 0; e < entries.length; e++) {
            out += entries[e].key + " = " + formatValue(entries[e].value) + "\n";
        }
    }
    return out;
}

function formatValue(v) {
    if (typeof v === "boolean") return v ? "true" : "false";
    if (typeof v === "number") return v.toString();
    return '"' + String(v).replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';
}

// ── Config ↔ QML Property Mapping ──────────────────────────────────────────
// Maps between flat dotted keys and the QML property names on Panel.qml.
// These same keys are used in daemon config_get/config_set JSON patches.

var CONFIG_MAP = {
    "prompt.layout":                      "cfgLayout",
    "prompt.transient":                   "cfgTransient",
    "prompt.newline":                     "cfgNewline",
    "prompt.right_prompt":                "cfgRightPrompt",
    "style.preset":                       "cfgStylePreset",
    "style.separators.left":              "cfgSepLeft",
    "style.separators.right":             "cfgSepRight",
    "prompt.newline":                     "cfgNewline",
    "prompt.blank_line":                  "cfgBlankLine",
    "style.frame.enabled":                "cfgFrameEnabled",
    "style.frame.gap_char":               "cfgFrameGapChar",
    "theme.source":                       "cfgThemeSource",
    "git.mode":                           "cfgGitMode",
    "git.enabled":                        "cfgGitEnabled",
    "git.branch_icon":                    "cfgGitBranchIcon",
    "segments.os.icon":                   "cfgOsIcon",
    "segments.character.success":         "cfgCharSuccess",
    "segments.character.error":           "cfgCharError",
    "segments.character.transient":       "cfgCharTransient",
    "segments.exit_status.show_signal_name": "cfgExitSignalNames",
    "segments.command_duration.show_above_ms": "cfgCmdDurationMs",
    "segments.ssh.show":                  "cfgSshShow",
    "segments.container.enabled":          "cfgContainerEnabled",
    "segments.python.enabled":             "cfgPythonEnabled",
    "segments.toolchain.enabled":          "cfgToolchainEnabled",
    "segments.nix.enabled":                "cfgNixEnabled",
    "segments.k8s.enabled":                "cfgK8sEnabled",
    "segments.time.enabled":               "cfgTimeEnabled",
    "segments.load.enabled":               "cfgLoadEnabled",
    "segments.time.format":                "cfgTimeFormat",
    "segments.battery.enabled":            "cfgBatteryEnabled",
    "segments.notification.threshold_ms":  "cfgNotifyThresholdMs",
    "terminal.title.enabled":              "cfgTitleEnabled",
};

function applyConfig(flat, target) {
    var keys = Object.keys(CONFIG_MAP);
    for (var i = 0; i < keys.length; i++) {
        var tomlKey = keys[i];
        var prop = CONFIG_MAP[tomlKey];
        var val = flat[tomlKey];
        if (flat.hasOwnProperty(tomlKey) && val !== undefined && val !== null)
            target[prop] = val;
    }
}

function collectConfig(source) {
    var flat = {};
    var keys = Object.keys(CONFIG_MAP);
    for (var i = 0; i < keys.length; i++) {
        var tomlKey = keys[i];
        var prop = CONFIG_MAP[tomlKey];
        flat[tomlKey] = source[prop];
    }
    return flat;
}

// ── JSON Config Flattening ─────────────────────────────────────────────────
// Converts nested JSON config from daemon into flat dot-keyed object.

function flattenConfig(obj, prefix) {
    var result = {};
    prefix = prefix || "";
    var keys = Object.keys(obj);
    for (var i = 0; i < keys.length; i++) {
        var k = keys[i];
        var full = prefix ? prefix + "." + k : k;
        var v = obj[k];
        if (v !== null && typeof v === "object" && !Array.isArray(v)) {
            var sub = flattenConfig(v, full);
            var subKeys = Object.keys(sub);
            for (var j = 0; j < subKeys.length; j++) {
                result[subKeys[j]] = sub[subKeys[j]];
            }
        } else if (v !== null) {
            result[full] = v;
        }
    }
    return result;
}

// Converts flat dot-keyed patch into nested JSON for config_set.
function unflattenPatch(flat) {
    var result = {};
    var keys = Object.keys(flat);
    for (var i = 0; i < keys.length; i++) {
        var parts = keys[i].split(".");
        var obj = result;
        for (var p = 0; p < parts.length - 1; p++) {
            if (!obj[parts[p]]) obj[parts[p]] = {};
            obj = obj[parts[p]];
        }
        obj[parts[parts.length - 1]] = flat[keys[i]];
    }
    return result;
}

// ── Daemon Response Parsing ────────────────────────────────────────────────

function parseDaemonResponse(json) {
    try { return JSON.parse(json); }
    catch (e) { return { error: "invalid JSON: " + e }; }
}

// Compares dotted protocol version strings (e.g. "0.3" >= "0.2").
function protocolAtLeast(current, min) {
    if (!current) return false;
    var cur = String(current).split(".").map(function (p) { return parseInt(p, 10) || 0; });
    var req = String(min).split(".").map(function (p) { return parseInt(p, 10) || 0; });
    var len = Math.max(cur.length, req.length);
    for (var i = 0; i < len; i++) {
        var c = i < cur.length ? cur[i] : 0;
        var r = i < req.length ? req[i] : 0;
        if (c > r) return true;
        if (c < r) return false;
    }
    return true;
}

// ── Tool Detection Parsing ─────────────────────────────────────────────────

function parseToolOutput(text) {
    var result = {};
    var lines = text.split("\n");
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim();
        if (line.length === 0) continue;
        var eqIdx = line.indexOf("=");
        if (eqIdx < 0) continue;
        var name = line.substring(0, eqIdx);
        var val = line.substring(eqIdx + 1);
        result[name] = (val === "missing") ? null : val;
    }
    return result;
}
