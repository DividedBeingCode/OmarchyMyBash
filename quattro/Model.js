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
