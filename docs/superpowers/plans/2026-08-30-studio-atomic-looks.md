# Studio: Atomic Looks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a preset apply as the atomic bundle it is presented as, make the glyph browser legible, split the three over-stacked Studio tabs, and grow the preset library from 29 to 52.

**Architecture:** A single `LOOK_OWNED` key set in `looks.rs` defines what a preset owns. `apply_look` clears those keys before merging a patch, and the preview path, the transient apply path and the persistent write path all call it — so the gallery matches Apply because it is one function, not two that agree. The QML work is independent: a sub-tab rail component, a glyph cell that sizes its glyph from its tile, and 23 new table entries.

**Tech Stack:** Rust (tokio, serde, toml), QML (Qt 6.11 / Quickshell), plain-JS test suites under `tests/`.

**Spec:** `docs/superpowers/specs/2026-08-30-studio-atomic-looks-design.md`

## Global Constraints

- Prompt render budget is **under 5 ms**; nothing added to `render.rs` or the prompt hot path.
- The daemon is the source of truth for anything rendered. Never reimplement a render or a color computation in QML.
- `qmllint` gate must stay green: `bash tests/qmllint.sh` (fails on non-zero exit, not just `^Error:` — syntax errors surface as `Warning: … [syntax]`).
- QML property names must not shadow `QQuickItem`/`PointerHandler` members: `palette`, `state`, `enabled`, `target` are all taken.
- `pragma ComponentBehavior: Bound` is required in a **file** component whose `Repeater` delegate reaches the root id. It must NOT be added to inline `Component {}` blocks.
- `Text.StyledText` collapses whitespace; newlines must be `<br/>`.
- Every commit message ends with the two trailer lines used throughout this repo (`Co-Authored-By:` and `Claude-Session:`).
- Full gate before any commit that touches Rust: `cargo test`. Before any commit that touches QML: `bash tests/qmllint.sh && bash tests/qml/run.sh`.

---

### Task 1: `LOOK_OWNED` and atomic merge

**Files:**
- Modify: `crates/omarchy10kd/src/looks.rs` (add constant + two functions, refactor `apply_transient`)
- Test: `crates/omarchy10kd/src/looks.rs` (`mod tests`)

**Interfaces:**
- Consumes: `crate::config::Config`, `crate::server::merge_toml_value`.
- Produces:
  - `pub const LOOK_OWNED: &[&str]`
  - `pub fn clear_look_owned(doc: &mut toml::Table)`
  - `pub fn apply_look(current: &Config, patch: &serde_json::Value) -> Result<Config, String>`
  - `pub fn apply_transient(current: &Config, patch: &serde_json::Value) -> Result<Config, String>` (unchanged signature, now delegates)

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/omarchy10kd/src/looks.rs`:

```rust
#[test]
fn clear_look_owned_removes_rather_than_writing_defaults() {
    // Removal, not explicit defaults: writing defaults would mark every
    // owned key as user-modified and break the panel's modified-vs-default
    // ink and its per-row reset.
    let value = toml::Value::try_from(Config::default()).expect("serialize");
    let mut doc = value.as_table().expect("table").clone();
    assert!(doc.contains_key("style"), "precondition");

    clear_look_owned(&mut doc);

    assert!(!doc.contains_key("style"), "style is Look-owned");
    let prompt = doc.get("prompt").and_then(|v| v.as_table()).expect("prompt survives");
    assert!(!prompt.contains_key("newline"), "prompt.newline is Look-owned");
    assert!(!prompt.contains_key("layout"), "prompt.layout is Look-owned");
    assert!(
        prompt.contains_key("right_prompt"),
        "prompt.right_prompt belongs to the user and must survive"
    );
    let git = doc.get("git").and_then(|v| v.as_table()).expect("git survives");
    assert!(!git.contains_key("branch_icon"), "git.branch_icon is Look-owned");
    assert!(git.contains_key("mode"), "git.mode belongs to the user");
}

#[test]
fn applying_a_look_resets_what_the_previous_look_set() {
    // The headline bug: framed-focus sets gap_gradient, lean-pure does not,
    // so under a delta merge lean-pure silently kept framed-focus's rule.
    let base = Config::default();
    let framed = apply_look(&base, &resolve("framed-focus", &base).expect("curated").patch)
        .expect("apply framed-focus");
    assert_eq!(framed.style.frame.gap_gradient.as_deref(), Some("off"));

    let lean = apply_look(&framed, &resolve("lean-pure", &base).expect("curated").patch)
        .expect("apply lean-pure");
    assert_eq!(
        lean.style.frame.gap_gradient,
        Config::default().style.frame.gap_gradient,
        "lean-pure inherited framed-focus's gap_gradient"
    );
}

#[test]
fn applying_a_look_leaves_your_own_settings_alone() {
    let mut base = Config::default();
    base.segments.battery.enabled = true;
    base.git.mode = "always".into();
    base.directory.max_length = 17;

    let after = apply_look(&base, &resolve("framed-focus", &base).expect("curated").patch)
        .expect("apply");

    assert!(after.segments.battery.enabled, "segment toggles are yours");
    assert_eq!(after.git.mode, "always", "git.mode is yours");
    assert_eq!(after.directory.max_length, 17, "directory settings are yours");
}

#[test]
fn apply_transient_still_merges_as_a_delta() {
    // The Look editor and project profiles genuinely want a delta; only
    // Apply is atomic.
    let base = Config::default();
    let framed = apply_transient(&base, &resolve("framed-focus", &base).expect("curated").patch)
        .expect("apply");
    let lean = apply_transient(&framed, &resolve("lean-pure", &base).expect("curated").patch)
        .expect("apply");
    assert_eq!(lean.style.frame.gap_gradient.as_deref(), Some("off"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p omarchy10kd looks:: 2>&1 | grep -E "^error|test result"`
Expected: FAIL — `cannot find function 'clear_look_owned'` and `cannot find function 'apply_look'`.

- [ ] **Step 3: Add the constant and the clear helper**

In `crates/omarchy10kd/src/looks.rs`, above `apply_transient`:

```rust
/// The config paths a Look owns.
///
/// Applying a Look CLEARS these before merging its patch, so a Look is the
/// atomic bundle it is presented as everywhere in the product. Without it a
/// patch is a delta: every key it omits inherits from whatever preset was
/// applied last, which is why 168 of 812 ordered (apply A, then B) pairs
/// rendered differently from B's own gallery card.
///
/// Deliberately excludes what belongs to the user rather than the preset:
/// segment enable/disable, `git.mode`, `directory.*`, `terminal.*`, plugins,
/// rice and statusline all survive an apply untouched.
///
/// `theme` is NOT here. It stays governed by the structure/complete rule: a
/// `complete` Look's patch carries a `theme` block (and the palette-replacement
/// rule below applies), a `structure` Look carries none and leaves your
/// palette alone.
pub const LOOK_OWNED: &[&str] = &[
    "style",
    "prompt.layout",
    "prompt.newline",
    "prompt.blank_line",
    "segments.os.icon",
    "segments.character.success",
    "segments.character.error",
    "segments.character.transient",
    "git.branch_icon",
];

/// Remove the Look-owned paths from a config table.
///
/// Removal rather than writing explicit defaults: an absent key reads as
/// "default" everywhere, keeps `config.toml` readable, and leaves the bar
/// popout's modified-vs-default ink honest. Writing defaults would mark every
/// owned key as user-modified.
pub fn clear_look_owned(doc: &mut toml::Table) {
    for path in LOOK_OWNED {
        clear_path(doc, path);
    }
}

fn clear_path(table: &mut toml::Table, path: &str) {
    match path.split_once('.') {
        None => {
            table.remove(path);
        }
        Some((head, rest)) => {
            if let Some(toml::Value::Table(inner)) = table.get_mut(head) {
                clear_path(inner, rest);
            }
        }
    }
}
```

- [ ] **Step 4: Refactor `apply_transient` into a shared body and add `apply_look`**

Replace the existing `pub fn apply_transient` with:

```rust
/// Merge a patch onto the current config as a DELTA — keys the patch omits
/// keep their current values. Used by the Look editor's working patch and by
/// project profiles, both of which genuinely want a delta.
pub fn apply_transient(current: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    merge_patch(current, patch, false)
}

/// Apply a Look ATOMICALLY: clear everything the Look owns, then merge its
/// patch. This is what makes a gallery card match what you get when you press
/// Apply — both go through here.
pub fn apply_look(current: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    merge_patch(current, patch, true)
}

fn merge_patch(
    current: &Config,
    patch: &serde_json::Value,
    atomic: bool,
) -> Result<Config, String> {
    let patch_val = serde_json::from_value::<toml::Value>(patch.clone())
        .map_err(|e| format!("look patch not representable in TOML: {e}"))?;
    let cur = toml::Value::try_from(current)
        .map_err(|e| format!("config serialize: {e}"))?;
    let mut doc = match cur.as_table() {
        Some(t) => t.clone(),
        None => toml::Table::new(),
    };
    if atomic {
        clear_look_owned(&mut doc);
    }
    // A patch that sets a palette sets the WHOLE palette. Without this the
    // deep merge leaves the previous palette's keys behind: switching from
    // Gruvbox (which ships an art-directed `ramp`) to a palette that derives
    // its own would keep Gruvbox's mustard ramp, and a partial user palette
    // would blend with whatever preceded it into a scheme nobody designed.
    if patch_val
        .get("theme")
        .and_then(|t| t.as_table())
        .is_some_and(|t| t.contains_key("custom"))
    {
        if let Some(theme) = doc.get_mut("theme").and_then(|t| t.as_table_mut()) {
            theme.remove("custom");
            theme.remove("ramp");
        }
    }
    if let Some(obj) = patch_val.as_table() {
        for (k, v) in obj {
            crate::server::merge_toml_value(
                doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())),
                v.clone(),
            );
        }
    }
    let text = toml::to_string(&doc).map_err(|e| format!("serialize: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("merged config invalid: {e}"))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p omarchy10kd 2>&1 | grep -E "test result|FAILED"`
Expected: PASS, and the whole suite still green (283 tests + the 4 new).

- [ ] **Step 6: Commit**

```bash
git add crates/omarchy10kd/src/looks.rs
git commit -m "feat(looks): LOOK_OWNED and atomic apply

A Look is presented everywhere as an atomic appearance bundle but was
applied as a delta, so every key a patch omitted inherited from whatever
preset was applied last. clear_look_owned removes the owned paths (rather
than writing defaults, which would mark them user-modified) and apply_look
merges onto that. apply_transient keeps delta semantics for the Look editor
and project profiles.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 2: Route every apply path through `apply_look`

**Files:**
- Modify: `crates/omarchy10kd/src/server.rs` (`effective_preview_config`, `looks_apply` handler, new `write_look_patch`)
- Test: `crates/omarchy10kd/src/server.rs` (`mod tests`)

**Interfaces:**
- Consumes: `crate::looks::{apply_look, clear_look_owned}` from Task 1.
- Produces: `async fn write_look_patch(state: &Arc<DaemonState>, patch: &serde_json::Value) -> Result<(), String>`

- [ ] **Step 1: Write the failing invariant test**

Add to `mod tests` in `crates/omarchy10kd/src/server.rs`:

```rust
/// Render a config the way a card and an applied prompt both render.
fn render_card_sized(cfg: &Config) -> String {
    let palette = crate::theme::ThemePalette::resolve_palette(cfg);
    let renderer = PromptRenderer::new(cfg, &palette);
    let git_status = crate::git::GitStatus {
        is_repo: true,
        branch: "main".into(),
        staged: 2,
        unstaged: 1,
        ..Default::default()
    };
    strip_np(&renderer.render_with_ssh(
        "~/app", 0, 0, 38, 0, &git_status, false, Some(false), None, Vec::new(),
    ).left).to_string()
}

#[test]
fn a_look_looks_the_same_applied_as_it_did_in_the_gallery() {
    // The headline invariant. Measured before the fix: 168 of these 812
    // ordered pairs differed, because the card was built atomically and the
    // apply was a delta.
    let base = Config::default();
    let names: Vec<String> =
        crate::looks::all(&base).iter().map(|d| d.name.clone()).collect();

    let mut mismatches: Vec<String> = Vec::new();
    for a in &names {
        let after_a = crate::looks::apply_look(
            &base, &crate::looks::resolve(a, &base).expect("curated").patch,
        ).expect("apply a");
        for b in &names {
            if a == b {
                continue;
            }
            let card = {
                let req = preview_req(None, Some(b));
                effective_preview_config(&req, &after_a, None).expect("card")
            };
            let applied = crate::looks::apply_look(
                &after_a, &crate::looks::resolve(b, &after_a).expect("curated").patch,
            ).expect("apply b");
            if render_card_sized(&card) != render_card_sized(&applied) {
                mismatches.push(format!("{a} then {b}"));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} ordered pairs differ, e.g. {:?}",
        mismatches.len(),
        names.len() * (names.len() - 1),
        &mismatches[..mismatches.len().min(5)]
    );
}

#[test]
fn the_leak_is_real_without_atomic_apply() {
    // Guards the guard. If delta apply ever stops leaking, the invariant
    // above has become vacuous and this fails to say so.
    let base = Config::default();
    let framed = crate::looks::apply_transient(
        &base, &crate::looks::resolve("framed-focus", &base).expect("curated").patch,
    ).expect("apply");
    let lean = crate::looks::apply_transient(
        &framed, &crate::looks::resolve("lean-pure", &base).expect("curated").patch,
    ).expect("apply");
    assert_eq!(
        lean.style.frame.gap_gradient.as_deref(),
        Some("off"),
        "delta apply no longer leaks; the invariant test may be vacuous"
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p omarchy10kd a_look_looks_the_same 2>&1 | tail -20`
Expected: FAIL — a non-empty mismatch list (`effective_preview_config` still uses `apply_transient` and still honors `base`).

- [ ] **Step 3: Point `effective_preview_config` at `apply_look`**

In `crates/omarchy10kd/src/server.rs`, inside `effective_preview_config`, replace the Look branch:

```rust
    if let Some(look_name) = &req.look {
        if let Some(l) = crate::looks::resolve(look_name, &effective) {
            // apply_look, not apply_transient: a card must show what pressing
            // Apply produces, and the only way to guarantee that is for both
            // to be the same function.
            effective = crate::looks::apply_look(&effective, &l.patch)?;
        }
    }
```

Leave the `req.patch` branch on `apply_transient` — a client patch (the Look
editor's working edit, the Theme tab's palette preview) is a genuine delta
over whatever is being previewed.

- [ ] **Step 4: Add `write_look_patch` and use it in `looks_apply`**

In `crates/omarchy10kd/src/server.rs`, beside `write_config_patch`:

```rust
/// Persist a Look atomically: clear the Look-owned keys from config.toml,
/// then merge the patch.
///
/// The FILE needs the same treatment as the in-memory config, or a daemon
/// restart resurrects exactly the leftovers the in-memory clear removed.
async fn write_look_patch(
    state: &Arc<DaemonState>,
    patch: &serde_json::Value,
) -> Result<(), String> {
    let config_path = state.config_path.clone();
    let mut doc: toml::Table = match std::fs::read_to_string(&config_path) {
        Ok(existing) => match toml::from_str(&existing) {
            Ok(t) => t,
            Err(e) => return Err(format!("config.toml has syntax errors: {e}")),
        },
        Err(_) => toml::Table::new(),
    };
    crate::looks::clear_look_owned(&mut doc);

    let mut failed_keys: Vec<String> = Vec::new();
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            match serde_json::from_value::<toml::Value>(v.clone()) {
                Ok(toml_val) => {
                    merge_toml_value(
                        doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())),
                        toml_val,
                    );
                }
                Err(e) => failed_keys.push(format!("{k} ({e})")),
            }
        }
    }
    if !failed_keys.is_empty() {
        return Err(format!(
            "values for keys {} are not representable in TOML",
            failed_keys.join(", ")
        ));
    }
    persist_config_doc(state, doc).await
}
```

In the `"looks_apply"` handler, change the two call sites:

- transient branch: `crate::looks::apply_transient(&current, &l.patch)` → `crate::looks::apply_look(&current, &l.patch)`
- persistent branch: `write_config_patch(state, &l.patch).await` → `write_look_patch(state, &l.patch).await`

- [ ] **Step 5: Run the tests**

Run: `cargo test -p omarchy10kd 2>&1 | grep -E "test result|FAILED"`
Expected: PASS. `a_look_looks_the_same_applied_as_it_did_in_the_gallery` reports zero mismatches.

- [ ] **Step 6: Commit**

```bash
git add crates/omarchy10kd/src/server.rs
git commit -m "fix(looks): apply a preset atomically everywhere

The preview path, the transient apply and the persistent write now all go
through apply_look, so a gallery card matches Apply because it is the same
function rather than two that happened to agree. write_look_patch gives
config.toml the same clear, or a restart resurrected the leftovers.

Pins the invariant across all 812 ordered pairs (168 differed before).

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 3: Delete the `base` preview field

**Files:**
- Modify: `crates/omarchy10kd/src/server.rs` (remove `PreviewRequest.base` and its uses)
- Modify: `quattro/o10k/Preview.js` (`cacheKey`), `quattro/Service.qml` (`requestPreview`), `quattro/StudioLooks.qml` (`_fetchCard`)
- Modify: `tests/preview_test.js`, `docs/wiki/protocol.md`

**Interfaces:**
- Consumes: atomic apply from Task 2 — this task is only safe once cards are stable without the flag.
- Produces: `requestPreview(look, patch, scenes, immediate, cb, cols)` (7th parameter gone); `cacheKey(look, patch, scenes, cols)`.

- [ ] **Step 1: Update the JS test first**

In `tests/preview_test.js`, delete the two `base`-specific blocks added earlier ("Card baseline") and replace with:

```javascript
// The card baseline flag is gone: applying a Look is atomic, so a card
// rendered on the live config is already stable across applies — and it also
// reflects the user's own segment toggles, which the default baseline hid.
(() => {
    const req = JSON.parse(P.buildRequest(
        { cwd: '~/app', cols: 38 }, null, 'synthwave', null, 'x1'));
    check('no baseline field is sent', Object.prototype.hasOwnProperty.call(req, 'base'), false);

    const a = P.cacheKey('synthwave', null, P.CARD_SCENES, 38);
    const b = P.cacheKey('synthwave', null, P.CARD_SCENES, 38);
    check('the same request shares a cache entry', a === b, true);
    check('width still separates renders',
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38) !==
          P.cacheKey('synthwave', null, P.CARD_SCENES, 80), true);
})();
```

- [ ] **Step 2: Run to verify it fails**

Run: `node tests/preview_test.js`
Expected: FAIL on "no baseline field is sent" is NOT expected (nothing sends base yet in that call); the run should pass trivially. Confirm instead that `grep -c base quattro/o10k/Preview.js` is non-zero — that is the state being removed.

- [ ] **Step 3: Remove `base` from the daemon**

In `crates/omarchy10kd/src/server.rs`:
- Delete the `pub base: Option<String>` field and its doc comment from `PreviewRequest`.
- Delete `base: None,` from the `preview_req` test helper.
- In `handle_preview`, drop `|| req.base.is_some()` from the branch condition.
- In `effective_preview_config`, replace the baseline block with `let mut effective = current.clone();`.
- Delete the tests `applying_a_look_does_not_rewrite_the_other_cards`, `the_leak_is_real_without_the_default_baseline`, and the `card_config` helper's `baseline` parameter — the Task 2 invariant test supersedes all three. Keep `a_structure_look_card_keeps_the_palette_you_are_on` and `a_complete_look_card_brings_its_own_palette`, retargeting their helper at `effective_preview_config` without a baseline.

- [ ] **Step 4: Remove `base` from the QML/JS side**

- `quattro/o10k/Preview.js`: `cacheKey(look, patch, scenes, cols, base)` → `cacheKey(look, patch, scenes, cols)`; drop `String(base || "")` and its comment from the joined key.
- `quattro/Service.qml`: `requestPreview(look, patch, scenes, immediate, cb, cols, base)` → drop `base`; `Preview.cacheKey(look, patch, scenes, useCols, base)` → drop it; `{ cwd: …, cols: useCols, base: base }` → `{ cwd: …, cols: useCols }`.
- `quattro/StudioLooks.qml`: in `_fetchCard`, drop the trailing `, "default"` argument and replace the `base: "default"` comment with:

```qml
        // Rendered on your LIVE config on purpose: applying a Look is atomic,
        // so a card is stable across applies without a synthetic baseline —
        // and this way the card also reflects segments you have switched off.
```

- [ ] **Step 5: Update the protocol doc**

In `docs/wiki/protocol.md`, delete the `| `base` | string | No | `"current"` | …` row from the preview request table.

- [ ] **Step 6: Run every gate**

```bash
cargo test 2>&1 | grep -E "test result|FAILED"
node tests/preview_test.js
bash tests/qmllint.sh
bash tests/qml/run.sh 2>&1 | tail -2
```
Expected: all pass; `grep -rn '"base"' crates/omarchy10kd/src/server.rs quattro/` returns nothing.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(preview): drop the base field, now redundant

base:\"default\" was a workaround for delta apply. With atomic apply a card
rendered on the live config is already stable across applies, and it also
reflects the user's own segment toggles, which the synthetic baseline hid.
One protocol field removed rather than added.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 4: Fix the glyph catalog and gate it

**Files:**
- Modify: `quattro/StudioPrompt.qml` (catalog entries, around lines 113–190)
- Modify: `crates/omarchy10kd/src/style.rs` (`mod catalog_parity_tests`)

**Interfaces:**
- Consumes: `keys_in_property(src, property)` and `qml(name)`, existing helpers in `catalog_parity_tests`.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing tests**

Add to `mod catalog_parity_tests` in `crates/omarchy10kd/src/style.rs`:

```rust
#[test]
fn the_studio_catalog_has_no_duplicate_keys() {
    // `dragon` shipped twice — once category "Animals", once "Japan" — so it
    // rendered twice in the grid.
    let src = qml("StudioPrompt.qml");
    let keys = keys_in_property(&src, "glyphCatalog");
    let mut seen = std::collections::BTreeSet::new();
    let dupes: Vec<&String> = keys.iter().filter(|k| !seen.insert((*k).clone())).collect();
    assert!(dupes.is_empty(), "duplicate glyph keys in the Studio catalog: {dupes:?}");
}

#[test]
fn every_glyph_the_daemon_resolves_is_browsable() {
    // kaomoji_relaxed, kaomoji_smirk and kaomoji_disapprove were resolvable
    // but not listed — and the shipped rose-classic Look uses
    // kaomoji_disapprove, so a preset depended on a glyph the picker could
    // not show.
    let src = qml("StudioPrompt.qml");
    let listed: std::collections::BTreeSet<String> =
        keys_in_property(&src, "glyphCatalog").into_iter().collect();
    let missing: Vec<&str> = available_symbol_chars()
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| !listed.contains(*k))
        .collect();
    assert!(missing.is_empty(), "daemon resolves glyphs the Studio cannot show: {missing:?}");
}
```

The property is `readonly property var glyphCatalog` at
`quattro/StudioPrompt.qml:112`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p omarchy10kd catalog_parity 2>&1 | grep -E "panicked|test result"`
Expected: FAIL twice — `duplicate glyph keys … ["dragon"]` and `daemon resolves glyphs the Studio cannot show: ["kaomoji_disapprove", "kaomoji_relaxed", "kaomoji_smirk"]`.

- [ ] **Step 3: Fix the catalog**

In `quattro/StudioPrompt.qml`:
- Delete the second `dragon` entry — the one with `category: "Japan"`. Keep the `Animals` one.
- Add the three missing kaomoji beside the existing kaomoji entries:

These are the daemon's exact strings, copied from
`crates/omarchy10kd/src/style.rs:616` onward — the two catalogs must agree
character for character or `every_glyph_the_daemon_resolves_is_browsable`
passes while the Studio shows a different glyph than the prompt renders:

```qml
        { key: "kaomoji_relaxed", glyph: "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}", label: "Relaxed", category: "Kaomoji" },
        { key: "kaomoji_smirk", glyph: "(\u{00ac}\u{203f}\u{00ac})", label: "Smirk", category: "Kaomoji" },
        { key: "kaomoji_disapprove", glyph: "\u{ca0}_\u{ca0}", label: "Disapprove", category: "Kaomoji" },
```

Verify against the source before committing:

```bash
grep -n 'kaomoji_relaxed\|kaomoji_smirk\|kaomoji_disapprove' crates/omarchy10kd/src/style.rs
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p omarchy10kd catalog 2>&1 | grep -E "test result|panicked"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add quattro/StudioPrompt.qml crates/omarchy10kd/src/style.rs
git commit -m "fix(studio): dedupe dragon, add the three unlisted kaomoji

dragon was declared under both Animals and Japan and rendered twice. Three
kaomoji the daemon resolves were never listed, and rose-classic uses one of
them — a shipped preset depending on a glyph the picker could not show. Both
are now gated.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 5: Make the glyph viewer legible

**Files:**
- Modify: `quattro/o10k/GlyphCell.qml`, `quattro/o10k/GlyphBrowser.qml`
- Test: `tests/qml/tst_glyphbrowser.qml` (create)

**Interfaces:**
- Consumes: the deduped catalog from Task 4.
- Produces: `GlyphCell.glyphSize`, `GlyphBrowser.category`.

- [ ] **Step 1: Write the failing test**

Create `tests/qml/tst_glyphbrowser.qml`:

```qml
import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "GlyphBrowser"

    property var sample: [
        { key: "cat",  glyph: "\u{f011b}", label: "Cat",  category: "Animals" },
        { key: "torii", glyph: "\u{f0705}", label: "Torii", category: "Japan" },
        { key: "chevron", glyph: "❯", label: "Chevron", category: "Prompt" }
    ]

    GlyphBrowser { id: browser; width: 640; catalog: sample }
    GlyphCell { id: cell; width: 80; glyph: "❯"; label: "Chevron" }

    function test_the_glyph_scales_with_its_tile() {
        // Shipped at a fixed 13px inside a ~64px tile: a glyph browser whose
        // whole purpose is showing what a glyph looks like rendered it at a
        // fifth of its own cell.
        verify(cell.glyphSize > 30, "glyph is " + cell.glyphSize + "px in an 80px tile")
        verify(cell.glyphSize < cell.width, "glyph cannot exceed its tile")
    }

    function test_tiles_are_big_enough_to_read() {
        verify(browser.columns <= 8, "columns = " + browser.columns)
    }

    function test_a_category_narrows_the_grid() {
        browser.category = "Japan"
        compare(browser.results.length, 1)
        compare(browser.results[0].key, "torii")
    }

    function test_all_restores_the_full_set() {
        browser.category = ""
        compare(browser.results.length, 3)
    }

    function test_category_and_query_combine() {
        browser.category = "Animals"
        browser.query = "cat"
        compare(browser.results.length, 1)
        browser.query = "torii"
        compare(browser.results.length, 0, "a query outside the category matches nothing")
        browser.query = ""
        browser.category = ""
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `bash tests/qml/run.sh 2>&1 | grep -A3 GlyphBrowser`
Expected: FAIL — `glyphSize` is undefined and `category` does not exist.

- [ ] **Step 3: Size the glyph from its tile**

In `quattro/o10k/GlyphCell.qml`, add the property and use it:

```qml
    /// Glyph size, derived from the tile rather than fixed.
    ///
    /// Was Style.font.subtitle — a flat 13px inside a ~64px tile, so the
    /// glyph took up about a fifth of its own cell. A browser whose entire
    /// job is showing how a glyph renders has to render it big enough to
    /// judge.
    property real glyphSize: Math.max(Style.font.subtitle, tile.width * 0.5)
```

and change the `Text`'s `font.pixelSize: Style.font.subtitle` to
`font.pixelSize: tile.glyphSize`.

Add the selected-tile name, overlaid so the grid does not reflow:

```qml
    Text {
        visible: tile.active && tile.label.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Style.space(3)
        width: parent.width - Style.space(6)
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        text: tile.label
        color: Color.background
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
    }
```

- [ ] **Step 4: Add the category filter and widen the tiles**

In `quattro/o10k/GlyphBrowser.qml`:
- `property int columns: 10` → `property int columns: 8`
- add `property string category: ""` beside `query`
- replace the `results` binding:

```qml
    readonly property var results: {
        var q = browser.query.trim().toLowerCase()
        var cat = browser.category
        var out = []
        for (var i = 0; i < browser.catalog.length; i++) {
            var e = browser.catalog[i]
            if (cat.length > 0 && String(e.category || "") !== cat)
                continue
            if (q.length > 0) {
                var hay = String(e.label || "") + " " + String(e.key || "")
                        + " " + String(e.category || "")
                if (hay.toLowerCase().indexOf(q) < 0)
                    continue
            }
            out.push(e)
        }
        return out
    }
```

- add a chip row above the search `Row`, inside `Column { id: layout }`:

```qml
        // 76 glyphs in one grid is a wall. The catalog already carries a
        // category per entry; these just surface it.
        Row {
            width: parent.width
            spacing: Style.space(6)

            Repeater {
                model: ["", "Prompt", "Animals", "Japan", "Kaomoji"]

                delegate: Chip {
                    required property string modelData
                    label: modelData.length === 0 ? "all" : modelData.toLowerCase()
                    active: browser.category === modelData
                    onClicked: browser.category = modelData
                }
            }
        }
```

- [ ] **Step 5: Run the gates**

```bash
bash tests/qmllint.sh
bash tests/qml/run.sh 2>&1 | grep -E "GlyphBrowser|Totals|FAIL"
```
Expected: lint passes, all five new cases pass.

- [ ] **Step 6: Look at it**

Install and screenshot — this project's rule is that visual bugs are found by looking, not reasoning:

```bash
rsync -a --delete quattro/ ~/.config/omarchy/plugins/community.omarchy10k/quattro/
omarchy restart shell && sleep 7
omarchy-shell shell summon community.omarchy10k && sleep 5
grim /tmp/glyphs.png
```

Confirm glyphs are large enough to identify and that the category chips filter.

- [ ] **Step 7: Commit**

```bash
git add quattro/o10k/GlyphCell.qml quattro/o10k/GlyphBrowser.qml tests/qml/tst_glyphbrowser.qml
git commit -m "fix(studio): glyphs were 13px inside 64px tiles

The glyph size is now derived from the tile (half its width) instead of a
fixed Style.font.subtitle, columns drop 10 to 8, and category chips turn one
76-tile wall into browsable subsets. The selected tile names itself without
reflowing the grid.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 6: Second-level rail on Prompt, Theme and System

**Files:**
- Create: `quattro/o10k/SubRail.qml`
- Modify: `quattro/StudioPrompt.qml`, `quattro/StudioTheme.qml`, `quattro/StudioSystem.qml`
- Test: `tests/qml/tst_subrail.qml` (create)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `SubRail` with `property var tabs` (array of strings), `property int current`, `signal switched(int index)`.

- [ ] **Step 1: Write the failing test**

Create `tests/qml/tst_subrail.qml`:

```qml
import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "SubRail"

    SubRail { id: rail; width: 400; tabs: ["Style", "Glyphs", "Segments"] }
    SubRail { id: empty; width: 400; tabs: [] }

    function test_it_starts_on_the_first_tab() {
        compare(rail.current, 0)
    }

    function test_an_out_of_range_index_is_clamped() {
        // A tab body keyed on `current` would render nothing at all for an
        // out-of-range index — a blank tab with no error anywhere.
        rail.current = 99
        compare(rail.current, rail.tabs.length - 1)
        rail.current = -3
        compare(rail.current, 0)
    }

    function test_an_empty_rail_does_not_go_negative() {
        compare(empty.current, 0)
        verify(!empty.visible, "a rail with nothing to switch between is hidden")
    }

    function test_switching_emits_once() {
        rail.current = 0
        var seen = []
        function record(i) { seen.push(i) }
        rail.switched.connect(record)
        rail.select(2)
        rail.switched.disconnect(record)
        compare(seen.length, 1)
        compare(seen[0], 2)
        compare(rail.current, 2)
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `bash tests/qml/run.sh 2>&1 | grep -A3 SubRail`
Expected: FAIL — `SubRail` is not a type.

- [ ] **Step 3: Create the component**

Create `quattro/o10k/SubRail.qml`:

```qml
pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "Fx.js" as Fx

// Second-level tab rail, inside a Studio tab.
//
// Three tabs stacked more than a screen: Prompt ran STYLE PRESET → SEPARATOR →
// PROMPT CHARACTER → ALL GLYPHS → BEHAVIOR, so the per-segment toggles — the
// controls changed most often — sat below a 76-tile glyph wall.
//
// Deliberately lighter than Studio.qml's top rail: underline rather than an
// accent fill, so the two levels read as a hierarchy instead of competing.
Item {
    id: rail

    /// Sub-tab labels, in order.
    property var tabs: []
    /// Selected index. Always clamped into range — a tab body keyed on this
    /// would render nothing at all for an out-of-range value, which looks
    /// like a blank tab with no error anywhere.
    property int current: 0

    signal switched(int index)

    onTabsChanged: rail.current = rail._clamp(rail.current)
    onCurrentChanged: {
        var c = rail._clamp(rail.current)
        if (c !== rail.current) rail.current = c
    }

    function _clamp(i) {
        if (!rail.tabs || rail.tabs.length === 0) return 0
        return Math.max(0, Math.min(rail.tabs.length - 1, i))
    }

    /// Select a sub-tab and notify. Setting `current` directly does not emit.
    function select(index) {
        var c = rail._clamp(index)
        rail.current = c
        rail.switched(c)
    }

    visible: rail.tabs && rail.tabs.length > 1
    implicitHeight: visible ? row.implicitHeight : 0
    height: implicitHeight

    Row {
        id: row
        spacing: Style.space(4)

        Repeater {
            model: rail.tabs

            delegate: Item {
                id: item
                required property string modelData
                required property int index

                implicitWidth: text.implicitWidth + Style.space(18)
                implicitHeight: text.implicitHeight + Style.space(14)

                readonly property bool isCurrent: rail.current === item.index

                Text {
                    id: text
                    anchors.centerIn: parent
                    text: item.modelData
                    color: item.isCurrent ? Color.foreground : Color.muted
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                    font.bold: item.isCurrent
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: Style.space(6)
                    anchors.rightMargin: Style.space(6)
                    height: 2
                    radius: 1
                    visible: item.isCurrent
                    color: Color.accent
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: rail.select(item.index)
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run the test**

Run: `bash tests/qml/run.sh 2>&1 | grep -E "SubRail|Totals|FAIL"`
Expected: PASS, four cases.

- [ ] **Step 5: Split the Prompt tab**

In `quattro/StudioPrompt.qml`, add `property int subTab: 0` on the root, put a
`SubRail` at the top of `Column { id: body }`:

```qml
        SubRail {
            width: parent.width
            tabs: ["Style", "Glyphs", "Segments"]
            current: promptTab.subTab
            onSwitched: (i) => promptTab.subTab = i
        }
```

Then wrap the existing section groups so each is visible only for its sub-tab.
Wrap `STYLE PRESET`, `SEPARATOR` and `PROMPT CHARACTER` (their heading `Text`
and the `Flow`/`Row` that follows each) in:

```qml
        Column {
            width: parent.width
            spacing: Style.space(12)
            visible: promptTab.subTab === 0
            // … the three existing section blocks, unchanged …
        }
```

`ALL GLYPHS` + its `GlyphBrowser` go in a `Column` with
`visible: promptTab.subTab === 1`; `BEHAVIOR` and its rows in one with
`visible: promptTab.subTab === 2`.

- [ ] **Step 6: Split the Theme and System tabs the same way**

Both follow the shape used for Prompt in Step 5. For
`quattro/StudioTheme.qml`, add `property int subTab: 0` to the root and at the
top of its `Column`:

```qml
        SubRail {
            width: parent.width
            tabs: ["Themes", "Palettes", "Gradient"]
            current: themeTab.subTab
            onSwitched: (i) => themeTab.subTab = i
        }
```

then wrap the existing blocks:

```qml
        Column {
            width: parent.width
            spacing: Style.space(12)
            visible: themeTab.subTab === 0
            // "OMARCHY THEMES" heading, its blurb, and the theme Flow
        }

        Column {
            width: parent.width
            spacing: Style.space(12)
            visible: themeTab.subTab === 1
            // "PIN TERMINAL COLORS" heading, its blurb, the search Row and
            // the palette Flow
        }

        Column {
            width: parent.width
            spacing: Style.space(12)
            visible: themeTab.subTab === 2
            // "GRADIENT" heading, the auto/full/off chips and the Ramp readout
        }
```

For `quattro/StudioSystem.qml`, identically, with
`property int subTab: 0`, `tabs: ["Sessions", "Plugins", "Layer"]`,
`current: systemTab.subTab`, and three `Column` wrappers around the
`SESSIONS` (0), `SEGMENT PLUGINS` (1) and `SHELL LAYER` (2) blocks. Use the
root id each file already declares — check it with
`grep -n "id: " quattro/StudioSystem.qml | head -1`.

- [ ] **Step 7: Run the gates and look at it**

```bash
bash tests/qmllint.sh
bash tests/qml/run.sh 2>&1 | tail -2
rsync -a --delete quattro/ ~/.config/omarchy/plugins/community.omarchy10k/quattro/
omarchy restart shell && sleep 7
omarchy-shell shell summon community.omarchy10k && sleep 5
grim /tmp/subrail.png
```

Check each sub-tab renders, the pinned preview still shows, and no tab is blank.

- [ ] **Step 8: Commit**

```bash
git add quattro/o10k/SubRail.qml quattro/StudioPrompt.qml quattro/StudioTheme.qml quattro/StudioSystem.qml tests/qml/tst_subrail.qml
git commit -m "feat(studio): second-level rail on Prompt, Theme and System

Prompt stacked STYLE PRESET, SEPARATOR, PROMPT CHARACTER, a 76-tile glyph
wall and then BEHAVIOR — so the per-segment toggles, the controls changed
most often, sat below several hundred pixels of glyphs. Each tab now splits
into three sub-tabs that fit a screen. The rail clamps its index, because a
tab body keyed on an out-of-range value renders blank with no error.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

### Task 7: 23 new Looks

**Files:**
- Modify: `crates/omarchy10kd/src/looks.rs` (`curated()` table)
- Modify: `README.md`, `docs/wiki/INDEX.md`, `docs/wiki/config.md` (counts and the Look-name list)

**Interfaces:**
- Consumes: `look(name, label, blurb, tags, patch)` and `with_palette(patch, palette_key)`, the existing table helpers.
- Produces: 23 new entries; `crate::looks::curated()` returns 52.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/omarchy10kd/src/looks.rs`:

```rust
#[test]
fn the_library_covers_its_own_glyph_families() {
    // 29 Looks drew on four glyph families and left Japan, sci-fi and most
    // kaomoji entirely unused.
    let all: Vec<String> = curated()
        .iter()
        .map(|l| serde_json::to_string(&l.patch).unwrap_or_default())
        .collect();
    let joined = all.join(" ");
    for glyph in ["torii", "sushi", "noodles", "sakura", "tea", "katana",
                  "alien", "robot", "ghost", "crown", "sword",
                  "kaomoji_shrug", "kaomoji_sleepy", "kaomoji_cheer"] {
        assert!(joined.contains(glyph), "no Look uses the {glyph} glyph");
    }
    assert_eq!(curated().len(), 52, "expected 52 curated Looks");
}

#[test]
fn every_look_name_is_unique() {
    let mut seen = std::collections::BTreeSet::new();
    let dupes: Vec<String> = curated()
        .iter()
        .filter(|l| !seen.insert(l.name.clone()))
        .map(|l| l.name.clone())
        .collect();
    assert!(dupes.is_empty(), "duplicate Look names: {dupes:?}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p omarchy10kd the_library_covers 2>&1 | grep -E "panicked|test result"`
Expected: FAIL — `no Look uses the torii glyph`.

- [ ] **Step 3: Add the Ukiyo and sci-fi Looks**

Append to the `curated()` vector in `crates/omarchy10kd/src/looks.rs`, before
the closing `]`. Every palette key, separator shape, preset name and glyph key
below was verified against `CURATED_PALETTES`, `available_separators`,
`available_presets` and `available_symbol_chars`.

```rust
        // ── Ukiyo: the Japan glyph family, unused until now ───────────────
        look("torii-dusk", "Torii Dusk",
            "Ink-wash blues and a gate at the end of the path.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "torii", "error": "torii", "transient": "torii" } },
                "git": { "branch_icon": "octicon" },
            }), "kanagawa")),
        look("sushi-bar", "Sushi Bar",
            "Muted rose, plain bars, one piece at a time.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sushi", "error": "kaomoji_rage", "transient": "sushi" } },
                "git": { "branch_icon": "text" },
            }), "rose-pine")),
        look("ramen-shop", "Ramen Shop",
            "Warm broth colors, dense as a full counter.",
            &["complete", "dense"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "noodles", "error": "kaomoji_rage", "transient": "noodles" } },
                "git": { "branch_icon": "octicon" },
            }), "gruvbox")),
        look("sakura-drift", "Sakura Drift",
            "Petals dissolving between segments.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sakura", "error": "sakura", "transient": "sakura" } },
                "git": { "branch_icon": "nerd" },
            }), "rose-pine-moon")),
        look("tea-house", "Tea House",
            "Green-grey calm and nothing you did not ask for.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "tea", "error": "tea", "transient": "tea" } },
                "git": { "branch_icon": "text" },
            }), "everforest")),
        look("steel-katana", "Steel Katana",
            "Cold blue-grey with a flame-cut edge.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "katana", "error": "katana", "transient": "katana" } },
                "git": { "branch_icon": "powerline" },
            }), "iceberg")),
        look("noh-mask", "Noh Mask",
            "Muted stage colors behind a framed rule.",
            &["complete", "framed", "two-line", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "slanted" }, "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "subtle" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "mask", "error": "drama", "transient": "mask" } },
                "git": { "branch_icon": "octicon" },
                "prompt": { "newline": true },
            }), "zenburn")),

        // ── Sci-fi ────────────────────────────────────────────────────────
        look("xenomorph", "Xenomorph",
            "Acid green on black, flame-cut. Something is in the vents.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "alien", "error": "alien", "transient": "alien" } },
                "git": { "branch_icon": "nerd" },
            }), "scarlet-protocol")),
        look("bot-farm", "Bot Farm",
            "Flat IBM Carbon, arrows, and no personality whatsoever.",
            &["complete", "powerline"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "robot", "error": "robot", "transient": "robot" } },
                "git": { "branch_icon": "powerline" },
            }), "oxocarbon")),
        look("ghost-shell", "Ghost Shell",
            "Segments that fade out before you finish reading them.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "ghost", "error": "ghost", "transient": "ghost" } },
                "git": { "branch_icon": "nerd" },
            }), "poimandres")),
        look("blue-cascade", "Blue Cascade",
            "Falling green on blue-black. Dense and unblinking.",
            &["complete", "dense"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
                "git": { "branch_icon": "text" },
            }), "blue-matrix")),
        look("deep-space", "Deep Space",
            "Navy with a magenta pulse, full rainbow segments.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "powerline" },
                "prompt": { "newline": true },
            }), "andromeda")),
```

- [ ] **Step 4: Add the expressive, regal and structure-only Looks**

Continue appending:

```rust
        // ── Expressive: the kaomoji family ────────────────────────────────
        look("shrug-life", "Shrug Life",
            "Bright and friendly, and completely unbothered by exit 1.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_shrug", "error": "kaomoji_shrug", "transient": "kaomoji_shrug" } },
                "git": { "branch_icon": "text" },
            }), "snazzy")),
        look("sleepy-dev", "Sleepy Dev",
            "Dusky blues for the 2am session.",
            &["complete", "minimal", "two-line"],
            with_palette(serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_sleepy", "error": "kaomoji_rage", "transient": "kaomoji_sleepy" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": true },
            }), "nightfox")),
        look("hype-machine", "Hype Machine",
            "Maximum voltage and a prompt that is thrilled for you.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_cheer", "error": "kaomoji_rage", "transient": "kaomoji_cheer" } },
                "git": { "branch_icon": "powerline" },
            }), "neon")),
        look("zen-mode", "Zen Mode",
            "The least prompt that is still a prompt.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "minimal", "separators": { "shape": "none" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_relaxed", "error": "kaomoji_relaxed", "transient": "kaomoji_relaxed" } },
                "git": { "branch_icon": "none" },
            }), "iceberg")),

        // ── Regal ─────────────────────────────────────────────────────────
        look("crown-jewels", "Crown Jewels",
            "Saturated violet with rounded caps.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "round" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "crown", "error": "crown", "transient": "crown" } },
                "git": { "branch_icon": "powerline" },
            }), "aura")),
        look("swordsman", "Swordsman",
            "Hot coral, slanted cuts, one clean stroke.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sword", "error": "sword", "transient": "sword" } },
                "git": { "branch_icon": "octicon" },
            }), "horizon")),

        // ── Structure only: your palette, a different shape ───────────────
        look("ascii-only", "ASCII Only",
            "No Nerd Font anywhere. For a console, an SSH session, or a tmux that lies about its font.",
            &["structure", "ascii-safe", "minimal"],
            serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dollar", "error": "dollar", "transient": "dollar" } },
                "git": { "branch_icon": "text" },
            })),
        look("single-line", "Single Line",
            "Everything on one row, no blank line above it.",
            &["structure", "minimal"],
            serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "octicon" },
                "prompt": { "newline": false, "blank_line": false },
            })),
        look("wide-load", "Wide Load",
            "Every segment, packed tight, thin arrows between.",
            &["structure", "dense", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "powerline_thin" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "linux" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "octicon" },
            })),
        look("round-trip", "Round Trip",
            "Powerline with rounded caps instead of arrows.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "round" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            })),
        look("diamond-cut", "Diamond Cut",
            "Faceted separators. Sharper than round, softer than flame.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "diamond" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "powerline" },
            })),
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p omarchy10kd 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: PASS. `patch_schema_tests` validates every new patch against
`Config::default()`, and the Task 2 invariant test now sweeps 52×51 = 2,652
ordered pairs.

- [ ] **Step 6: Update the counts in the docs**

- `README.md`: `28 curated Looks and 53 palettes` → `52 curated Looks and 53 palettes`; the Looks tab row `Browse 28 presets` → `Browse 52 presets`; the Status block `370 unit tests` → the number `cargo test` now reports.
- `docs/wiki/INDEX.md` and `docs/wiki/config.md`: update any Look count and the
  Look-name list near `config.md:202`.

Find every stale count with:

```bash
grep -rn "28 curated\|29 presets\|28 presets\|28 Looks" README.md docs/
```

- [ ] **Step 7: Look at it**

```bash
cargo build --release
for b in omarchy10k omarchy10kd; do install -m755 target/release/$b ~/.local/bin/.$b.new && mv -f ~/.local/bin/.$b.new ~/.local/bin/$b; done
pkill -f omarchy10kd; sleep 2; omarchy10k prompt >/dev/null 2>&1
omarchy restart shell && sleep 7
omarchy-shell shell summon community.omarchy10k && sleep 6
grim /tmp/looks52.png
```

Check the new cards render with their own palettes and glyphs, and that no
card shows tofu.

- [ ] **Step 8: Commit**

```bash
git add crates/omarchy10kd/src/looks.rs README.md docs/
git commit -m "feat(looks): 23 new presets, 29 to 52

The library drew on four glyph families and left Japan, sci-fi and most
kaomoji unused. Adds Ukiyo, sci-fi, expressive and regal sets plus five
structure-only shapes, including an ascii-safe Look for a console with no
Nerd Font and single-line for people who do not want two rows.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Gp8LoJNfJnZ15kyUHeovPJ"
```

---

## Final verification

- [ ] **Run every gate**

```bash
cargo test 2>&1 | grep -E "test result"
bash tests/qmllint.sh
bash tests/qml/run.sh 2>&1 | tail -2
for f in tests/*_test.js; do node "$f" >/dev/null && echo "ok $(basename $f)"; done
bash tests/integration_test.sh 2>&1 | grep -E "Results"
```

Expected: all green; integration reports `0 failed`.

- [ ] **Refresh the README screenshots**

`docs/img/studio-looks.png` and `docs/img/studio-theme.png` both predate the
sub-rails and the 52-Look grid. Recapture with `grim` and crop to the Studio
window as the previous ones were.
