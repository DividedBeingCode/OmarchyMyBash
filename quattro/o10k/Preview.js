.pragma library

// Preview scenes, request building, and hover coalescing.
//
// A .pragma library so the logic is node-testable (tests/preview_test.js).
// The QML side owns the socket and the Timer; everything decidable without
// them lives here, which is the same split Store.js uses and the reason the
// broker logic could be tested at all.

// ── Scene catalog ──────────────────────────────────────────────────────────
//
// What a prompt actually has to survive. A preview that shows only a clean
// repo is not a preview: the interesting question about a Look is what it
// does when the branch is dirty, the last command failed, the path is deep,
// or you are on a remote host — and those are exactly the states you cannot
// see by staring at a picker.
//
// Ordered so the eye reads the common case first and the failure case in the
// middle, where it is unmissable.

var SCENES = [
    {
        key: "clean",
        label: "clean repo",
        git_branch: "main",
        cmd_duration_ms: 0,
        exit_code: 0
    },
    {
        key: "dirty",
        label: "uncommitted work",
        git_branch: "main",
        git_staged: 2,
        git_unstaged: 1,
        exit_code: 0
    },
    {
        key: "failed",
        label: "command failed",
        git_branch: "main",
        exit_code: 127,
        cmd_duration_ms: 2400
    },
    {
        key: "deep",
        label: "deep path",
        cwd: "~/code/acme/backend/services/auth",
        git_branch: "feature/oauth-refresh",
        exit_code: 0
    },
    {
        key: "ssh",
        label: "over ssh",
        cwd: "~/dotfiles",
        git_branch: "main",
        in_ssh: true,
        exit_code: 0
    },
    {
        key: "plain",
        label: "no repo",
        cwd: "~",
        git_branch: "",
        exit_code: 0
    }
];

/// The subset shown on a compact card, where six rows would not fit.
///
/// Rendered narrow on purpose. A card is roughly a third the width of the
/// preview pane, and at the pane's column count the daemon pads frame rules
/// and right-aligned segments out past the card edge, so every card elided
/// mid-prompt -- which is the exact failure the old gallery had.
var CARD_SCENES = [{
    key: "dirty",
    label: "",
    // A SHORT path on purpose. A card is a thumbnail roughly 38 columns
    // wide, and "~/projects/my-app" spends most of that budget on the path
    // alone -- at which point the daemon correctly drops it to fit the
    // frame, and every framed preset renders as a bare rule with no path at
    // all. The point of the card is the preset's SHAPE: its separators, its
    // glyphs, its colours.
    cwd: "~/app",
    git_branch: "main",
    git_staged: 2,
    git_unstaged: 1,
    exit_code: 0,
    cols: 44
}];

/// The card scene rendered to a specific width.
///
/// A fixed 44 was still too wide: three cards across a 1440 canvas are about
/// 274px, roughly 35 columns, so every card wrapped onto a second line. Ask
/// for what the card can show and it fits by construction.
function cardScenes(cols) {
    var out = [];
    for (var i = 0; i < CARD_SCENES.length; i++) {
        var s = {};
        for (var k in CARD_SCENES[i]) s[k] = CARD_SCENES[i][k];
        if (cols && cols > 0) s.cols = cols;
        out.push(s);
    }
    return out;
}

/// Two scenes at bar-popout width. The bar panel is 360px wide, so the
/// Studio's six rows at 88 columns do not fit; these two cover the states
/// worth seeing in a glance-sized surface.
var PANEL_SCENES = [
    { key: "clean", label: "clean", git_branch: "main", exit_code: 0, cols: 40 },
    { key: "dirty", label: "dirty", git_branch: "main",
      git_staged: 2, git_unstaged: 1, exit_code: 0, cols: 40 }
];

/// Fields a scene may carry. Anything else is dropped rather than sent, so a
/// typo in a scene definition fails here instead of being silently ignored by
/// the daemon's `#[serde(default)]` fields.
var SCENE_FIELDS = [
    "cwd", "exit_code", "cmd_duration_ms", "cols", "jobs",
    "in_ssh", "git_branch", "git_staged", "git_unstaged", "label"
];

function _cleanScene(scene) {
    var out = {};
    for (var i = 0; i < SCENE_FIELDS.length; i++) {
        var f = SCENE_FIELDS[i];
        if (scene[f] !== undefined && scene[f] !== null)
            out[f] = scene[f];
    }
    return out;
}

/// Build one NDJSON preview request line.
///
/// `ctx` carries the shared render context (cwd, cols); `patch` and `look`
/// are the thing being previewed; `scenes` is the catalog subset to render.
///
/// The trailing newline is NOT optional. The daemon reads NDJSON line by
/// line, so a request without it is never dispatched — a fetcher that
/// trimmed this newline is why the Looks list silently stayed empty.
function buildRequest(ctx, patch, look, scenes, id) {
    var msg = { type: "preview" };
    if (ctx) {
        for (var k in ctx) {
            if (ctx[k] !== undefined && ctx[k] !== null)
                msg[k] = ctx[k];
        }
    }
    if (patch !== undefined && patch !== null)
        msg.patch = patch;
    if (look !== undefined && look !== null && String(look).length > 0)
        msg.look = String(look);
    if (scenes && scenes.length > 0) {
        var out = [];
        for (var i = 0; i < scenes.length; i++)
            out.push(_cleanScene(scenes[i]));
        msg.scenes = out;
    }
    if (id) msg.id = id;
    return JSON.stringify(msg) + "\n";
}

/// Stable cache key for a preview request.
///
/// `cols` is part of the key. Without it a render made for a narrow pane is
/// served back to a wide one and vice versa -- the prompt is laid out to a
/// column count, so two widths are two different renders.
function cacheKey(look, patch, scenes, cols) {
    var sceneKeys = [];
    if (scenes) {
        for (var i = 0; i < scenes.length; i++)
            sceneKeys.push(scenes[i].key || scenes[i].label || String(i));
    }
    return [String(look || ""), _stableStringify(patch),
            sceneKeys.join(","), String(cols || "")].join("|");
}

function _stableStringify(value) {
    if (value === undefined || value === null)
        return "null";
    if (typeof value !== "object")
        return JSON.stringify(value);
    if (Array.isArray(value)) {
        var items = [];
        for (var i = 0; i < value.length; i++)
            items.push(_stableStringify(value[i]));
        return "[" + items.join(",") + "]";
    }
    var keys = Object.keys(value).sort();
    var parts = [];
    for (var j = 0; j < keys.length; j++)
        parts.push(JSON.stringify(keys[j]) + ":" + _stableStringify(value[keys[j]]));
    return "{" + parts.join(",") + "}";
}

// ── Hover coalescing ───────────────────────────────────────────────────────

/// A debouncer driven by an injected clock and scheduler.
///
/// Returned as a closure rather than built on a QML Timer so the timing is
/// testable without a running QML engine — the same reason Store.js exists.
/// The QML side passes `Motion.MICRO_MS` and a Timer-backed scheduler.
///
/// Crossing a grid of eighteen cards fires eighteen hover events; without
/// this that is eighteen daemon round-trips for a preview nobody looked at.
function debouncer(delayMs, schedule, cancel) {
    var pending = null;
    var handle = null;

    function fire() {
        handle = null;
        var run = pending;
        pending = null;
        if (run) run();
    }

    return {
        /// Queue `fn`, replacing anything already queued.
        request: function (fn) {
            pending = fn;
            if (handle !== null && cancel) cancel(handle);
            handle = schedule(fire, delayMs);
        },
        /// Run the queued call now — for a click, which should never wait.
        flush: function () {
            if (handle !== null && cancel) cancel(handle);
            fire();
        },
        /// Drop the queued call — for a mouse leaving the grid.
        clear: function () {
            if (handle !== null && cancel) cancel(handle);
            handle = null;
            pending = null;
        },
        pending: function () { return pending !== null; }
    };
}
