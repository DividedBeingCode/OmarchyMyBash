# Packaging an Omarchy Plugin

A general reference for shipping a third-party Omarchy 4 (Quattro) plugin — especially one that is *also* a compiled program.

Everything here was verified by reading Omarchy's own source
(`/usr/share/omarchy/bin/omarchy-plugin-*`,
`/usr/share/omarchy/shell/services/PluginRegistry.qml`,
`/usr/share/omarchy/shell/shell.qml`) and by running the commands on a live
Omarchy 4 machine. Where a rule cost someone a bug, the bug is named.

---

## 1. The one thing that surprises everybody

**`omarchy plugin add` never builds anything.**

Read the source and it is explicit: clone the repo, validate the manifest, move
it into place, rescan, optionally enable. No build step, no install hook, no
`sudo`. That is a deliberate security posture — the command warns you that
plugins run *unsandboxed inside a long-lived shell process* — and it is not
going to change.

So if your plugin needs a binary on `PATH`, `omarchy plugin add` gives your user
**half a product**. Whether they can tell is up to you.

The worst version of this: the QML degrades silently. A plugin whose surfaces
are gated on `visible: someService.connected` renders *nothing at all* — no
window, no error, no hint. The user's best available conclusion is that your
plugin is broken; the likely one is that they never notice it installed. If
your plugin has an external dependency, show something on screen that says so.

---

## 2. Manifest rules the validator enforces

`omarchy plugin validate <folder>` mirrors `PluginRegistry.qml`, so anything it
rejects, the shell would too. Run it before you publish — it exits 0 on success
and is the cheapest check you will ever run.

| Rule | Detail |
|---|---|
| `manifest.json` at the **repo root** | Not in a subdirectory — a payload in `quattro/` or `plugin/` cannot be added. See the worked examples. |
| `schemaVersion` | Exactly the JSON **number** `1`. The string `"1"` is rejected. |
| Required fields | `id`, `name`, `version`, `kinds`, `entryPoints` |
| `id` charset | `^[A-Za-z0-9][A-Za-z0-9._-]*$`, and no `..` |
| `id` namespace | **`omarchy.*` is reserved.** Use `vendor.name` — e.g. `community.omarchy10k`, `ijohnst.spatial-ux`. |
| `kinds` | Non-empty array |
| `entryPoints` | Object; every value a **relative** path with no `..`, no newline, and **the file must exist** |
| Kind → entry point | Declaring a kind without its entry point is refused (table below) |
| Symlinks | **None anywhere** in the folder, except inside `.git` |

Kind → required entry-point key:

| kind | entryPoints key |
|---|---|
| `bar` | `bar` |
| `bar-widget` | `barWidget` |
| `menu` | `menu` |
| `overlay` | `overlay` |
| `panel` | `panel` |
| `service` | `service` |

The symlink rule bites during development: a "just symlink my checkout into the
plugins dir" workflow produces a folder the CLI validator refuses. The running
shell's registry does **not** check symlinks, so the symlink still *works* at
runtime — which means you can develop happily for weeks and only discover the
problem when someone tries to `omarchy plugin add` your repo.

### Bar widgets carry a settings schema

A `bar-widget` needs a `barWidget` block, and it does more than name the thing.
From Omarchy's own `shell/README.md`:

```json
"barWidget": {
  "displayName": "Cool clock",
  "category": "Time",
  "allowMultiple": false,
  "defaultSection": "left",
  "defaults": { "format": "HH:mm" },
  "schema": [ { "key": "format", "type": "string", "label": "Format" } ]
}
```

`defaults` and `schema` are what give your widget **user-editable settings in
the Omarchy UI** rather than a config file only you know about.
`defaultSection` must be `left`, `center` or `right` (the validator checks) and
falls back to centre when omitted; the user can move it afterwards with
`omarchy bar move`. Only one `bar` (full-bar replacement) plugin is active at a
time, and there is always a fallback to `omarchy.bar`.

### `keepLoaded` is real, and it decides whether you exist

`keepLoaded: true` is not in the validator's required list but the shell reads
it. `panel`, `overlay` and `menu` kinds are all mounted through one `Loader`:

```qml
active: sourceUrl !== "" && (keepLoaded || openPanelIds[pluginId] === true)
```

So an `overlay` **without** `keepLoaded: true` is only mounted while the shell
considers its panel open. If your overlay is a thing the user summons (a picker,
a studio window), that is exactly right — leave it off. If your overlay is
always-on chrome, omit `keepLoaded` and it will never mount at all.

### What the shell injects

On load, the shell sets these on your root item if the properties exist:
`omarchyPath`, `shell`, `manifest`, `barWidgetRegistry`, `pluginRegistry`, and
`service` (the singleton from your own `service` entry point, if you declared
one). Declare the ones you want; the rest are ignored.

---

## 3. The directory name must equal the manifest id

This one is quiet and nasty.

`omarchy plugin add` clones to `$PLUGINS_DIR/<id>`. So do `omarchy plugin
remove` and `omarchy plugin update` — they resolve a plugin as
`$PLUGINS_DIR/<id>` and **never read manifests to find it**. Only
`omarchy-plugin-catalog` walks directories and reads manifests.

The result of installing to any other directory name is a plugin that:

- appears in `omarchy plugin list` ✓
- loads and runs ✓
- and reports **"plugin '<id>' is not installed"** from `omarchy plugin update`

Spatial UX shipped this way for a long time: manifest id `ijohnst.spatial-ux`,
installed by its own script to `plugins/spatial-ux`. Everything worked except
the plugin manager, and nothing said so.

**If you have an installer script, make it write `$PLUGINS_DIR/<manifest id>`,
and migrate the old directory on sight** so users are not left with the plugin
registered twice.

This is not an inference from the CLI's behaviour — it is the documented
procedure. Omarchy's `shell/README.md` describes installing by hand as:

> 1. Put it in `~/.config/omarchy/plugins/<plugin-id>/` with a `manifest.json`
>    plus the QML referenced from its `entryPoints`.
> 2. `omarchy-shell shell rescanPlugins`.
> 3. `omarchy plugin enable <id>`.

That is exactly what an installer for a compiled plugin should do for its
per-user half.

---

## 4. Two discovery roots, one of them off-limits

| Root | Who |
|---|---|
| `$OMARCHY_PATH/shell/plugins` | Omarchy's own. Package-owned; wiped on `omarchy update`. Never write here. |
| `~/.config/omarchy/plugins` | Everyone else |

`firstParty` is computed from the id prefix (`omarchy.`), not from the
directory — but the reserved-id check already stops you claiming it, and the
skill rule against writing to `/usr/share/omarchy/` stops you living there.

**A system package therefore cannot install a plugin.** There is nowhere for it
to go: the QML must land in a user's home, and only a per-user step can put it
there. This is the constraint that shapes everything below.

---

## 5. Lifecycle facts worth knowing before you ship

**Plugins land disabled.** `omarchy plugin add` clones the repo and stops
there unless you pass `--enable` — deliberately, so the user can read the code
before it runs inside their shell. Your README should say which it expects.

**`--yes` is the documented path for scripts and agents.** Every `omarchy
plugin` command is interactive when run bare in a terminal (gum pickers, a
confirmation, a diff to review) and non-interactive when given arguments.
Omarchy's own README calls `--yes` "the path for scripts and AI agents":

```bash
omarchy plugin add https://github.com/acme/omarchy-weather.git --enable --yes
omarchy plugin update --yes
```

**Updates are a fast-forward of a git checkout.** `omarchy plugin update <id>`
fetches, shows a diff, and fast-forwards — and it **refuses when the checkout
has local changes**. Two consequences: a plugin installed by copying files
(no `.git`) cannot be updated by the plugin manager at all, and a user who
edited your plugin in place is stuck until they stash. Anything beyond
add/update — pinning a ref, switching branches — is ordinary git in the plugin
directory.

**Code hot-reloads on save.** Saving any file under
`~/.config/omarchy/plugins/` reloads plugin code automatically;
`omarchy-shell shell rescanPlugins` forces it.

> **Caveat found the hard way.** Hot reload re-evaluates your QML, but a
> *nested* `PanelWindow` — a second layer-shell window your plugin declares
> inside its entry point — can survive the reload with the old code still in
> it. The log says the plugin reloaded, nothing errors, and your change simply
> has no effect, which is indistinguishable from a change that does not work.
> If you have one, verify against `omarchy restart shell` before concluding
> your edit was wrong.

**To modify a built-in, clone it — never edit `$OMARCHY_PATH`.**
`omarchy plugin clone omarchy.clock` copies it to `<username>.clock` in your
user plugin dir and switches the bar over, preserving position and settings.

---

## 6. If your plugin ships a binary

Split along the only line available:

| Piece | Owner |
|---|---|
| Compiled binaries, CLI, shared assets | an Arch package → `/usr/bin`, `/usr/share/<name>` |
| QML plugin, user config, hooks, keybinds | a per-user setup step |

### Ship one script, run it in two modes

The tempting shortcut is to write the per-user logic twice — once in your
repo's `install.sh` and once inside the package. Don't. The two copies drift,
and the drift surfaces as *"works from source, not from the package"*, which is
among the worst bugs to reproduce.

Instead, **package your own `install.sh`** into `/usr/share/<name>/` with a
thin `/usr/bin/<name>-setup` wrapper that runs it in a package mode. Spatial UX
does this with three changes in that mode:

- a `SRC_ROOT` variable points at `/usr/share/<name>` instead of the repo
- the build and binary-install steps are skipped — pacman owns those files, and
  writing them again shadows tracked files with untracked ones
- asset copying is skipped if your asset lookup already has the system path in
  its fallback chain

Make the share root overridable by an environment variable. That single line is
what lets you test the packaged path against an *extracted* package without
installing anything system-wide — and a packaging change you cannot test is a
packaging change that is not known to work.

### Get the PKGBUILD right, not plausible

Follow the [Arch Rust package guidelines](https://wiki.archlinux.org/title/Rust_package_guidelines)
literally: `cargo fetch --locked --target "$(rustc -vV | sed -n 's/^host: //p')"`
in `prepare()`, then `--frozen` in `build()` and `check()` so both run offline.
Add `--workspace` to `check()` if your root `Cargo.toml` is a virtual manifest.

Two mistakes that are easy to ship because the package still builds and
installs cleanly:

- **`depends` that lists what you *think* it needs.** A first draft here
  declared `hyprland` and `quickshell` — the functional dependencies — and none
  of the libraries the binaries actually link. Derive the list instead:

  ```bash
  for lib in $(ldd target/release/<bin> | awk '{print $3}' | grep '^/'); do
      pacman -Qoq "$lib"
  done | sort -u
  ```

  That turned up `alsa-lib`, `dbus` and `systemd-libs`, none of which had been
  declared. (`namcap <pkg>.tar.zst` does this and more if you have it.)
- **A source directory derived from the repository name.** `source=("git+$url")`
  checks out into a directory named after the remote repo, so `cd` in each
  function guesses at that name and breaks the day the repo is renamed. Name
  the source instead — `source=("$pkgname::git+$url#tag=v$pkgver")` — and the
  checkout always lands at `$srcdir/$pkgname`.

### Post-install messaging

`.install` hooks are the right place to tell the user about the remaining step.
`post_install` should name the setup command; `pre_remove` should tell them to
run your uninstaller **before** removing the package, while the files it needs
still exist.

---

## 7. Blockers people hit at the last minute

**No LICENSE file.** A repo with no license is all-rights-reserved by default,
which makes redistribution unlawful for whoever receives it. An AUR `license=`
field cannot be guessed on the author's behalf. Decide this early; it is the
one item on this list that nobody else can do for you.

**`source=` points at a tag that does not exist.** A `PKGBUILD` sourcing
`git+<url>#tag=v$pkgver` needs that tag pushed.

**The package ships `HEAD`, not your working tree.** Obvious in principle,
invisible in practice. Building Spatial UX's package produced an artifact
missing three QML files that every surface imports, and still containing one
that had been deleted — because that work was uncommitted. **Build the package
and list its contents** before you believe it:

```bash
makepkg --nocheck
tar -tf *.pkg.tar.zst | sort
```

Diff that listing against your working tree. It takes a minute and it is the
only way to catch this class of mistake.

---

## 8. Where a plugin is actually distributed

| Channel | Reality |
|---|---|
| **A public git repo** | The primary and only channel Omarchy itself supports for plugins. `omarchy plugin add <url>`. |
| **AUR**, for the binary half | The realistic route for a compiled dependency. Install with `omarchy pkg aur add <pkg>` — note `aur`: plain `omarchy pkg add` is `pacman -S` and will not find it. |
| **Omarchy Package Repository (OPR)** | Real and already a configured pacman repo (`pkgs.omarchy.org`, visible in `/etc/pacman.conf`), built from `omacom/omarchy-pkgs`. There is **no documented third-party submission path**, so do not plan on it. |
| **omarchyplugins.com** | A community directory. Note it self-describes as "an independent community project. Not affiliated with, sponsored by, or endorsed by Omarchy or 37signals," and was still near-empty when checked. Listing there is discovery, not distribution. |

There is no official plugin store, and no mechanism by which adding a plugin
installs a package. If your plugin needs a binary, the user performs two
actions, and your job is to make the second one obvious.

## 9. A checklist

```bash
omarchy plugin validate .                 # exits 0, or tells you exactly what is wrong
jq -r .id manifest.json                   # your install dir must equal this
find . -name .git -prune -o -type l -print   # must be empty
```

- [ ] `manifest.json` at the repo root, `schemaVersion: 1` as a number
- [ ] `id` is `vendor.name`, not `omarchy.*`
- [ ] every declared kind has its entry-point key, and each file exists
- [ ] no symlinks outside `.git`
- [ ] `keepLoaded` set if and only if your overlay/panel is always-on
- [ ] installer writes `$PLUGINS_DIR/<manifest id>` and migrates older names
- [ ] if you ship a binary: the plugin says something on screen when it is missing
- [ ] LICENSE file present, and the manifest's `license` field matches
- [ ] `barWidget.defaults` + `schema` present if you want user-editable settings
- [ ] README says whether to pass `--enable`, and names any binary dependency
- [ ] `depends` derived from `ldd`, not from memory
- [ ] source named `$pkgname::git+...` so `$srcdir/$pkgname` is predictable
- [ ] package built and its file listing diffed against the working tree
- [ ] the whole flow tried on a clean-ish path: `omarchy plugin add <url> --enable --yes`

---

## Sources

Re-verify rather than trust this file; all of it is on disk:

| Claim | Where |
|---|---|
| Manifest schema, kinds, `keepLoaded`, hand-install, IPC contract | `/usr/share/omarchy/shell/README.md` |
| Validator rules | `/usr/share/omarchy/bin/omarchy-plugin-validate` |
| Never builds; clone → validate → move | `/usr/share/omarchy/bin/omarchy-plugin-add` |
| Directory = id for remove/update | `/usr/share/omarchy/bin/omarchy-plugin-{remove,update}` |
| Discovery roots, first-party flag | `/usr/share/omarchy/shell/plugins/README.md`, `omarchy-plugin-catalog` |
| Panel/overlay/menu loader and `keepLoaded` | `/usr/share/omarchy/shell/shell.qml` (`computePanelEntries`) |
| `pkg add` vs `pkg aur add` | `/usr/share/omarchy/bin/omarchy-pkg-{add,aur-add}` |
| OPR is a configured repo | `/etc/pacman.conf` → `[omarchy]` |

Online: the [Omarchy manual on shell plugins](https://omarchy.org/manual/shell-plugins)
and [`omacom/omarchy-pkgs`](https://github.com/omacom/omarchy-pkgs) for how
Omarchy builds its own packages.

## Worked examples on this machine

**`community.omarchy10k`** — QML plugin plus an `omarchy10k` Rust binary in
`~/.local/bin`. Manifest is exemplary: correct namespaced id, four kinds each
with an entry point, a `license` field, `barWidget.defaultSection`. No
`keepLoaded`, which is right — its overlay and panel are both summoned.

Its one blocker for `omarchy plugin add` is §2's first row: **`manifest.json`
lives in `quattro/`, not at the repo root**, so `omarchy plugin validate` on the
repo root fails with *"missing manifest.json"* and `omarchy plugin add` would
refuse it. Three ways out: move the plugin payload to the repo root (what
Spatial UX did), publish a separate repo containing only the payload, or skip
`omarchy plugin add` entirely and distribute via package + installer.

**`ijohnst.spatial-ux`** — QML plugin plus a Rust daemon. Repo root *is* the
plugin dir, so `omarchy plugin add` works. Ships a `PKGBUILD` splitting daemon
(package) from plugin (per-user), with the package carrying its own
`install.sh` so both paths run identical code.
