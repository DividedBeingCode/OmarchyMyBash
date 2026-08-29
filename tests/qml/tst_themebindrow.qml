import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "ThemeBindRow"

    ThemeBindRow {
        id: bound
        width: 340
        cfgFlat: ({ "theme.source": "omarchy" })
        desktopTheme: "tokyo-night"
    }

    ThemeBindRow {
        id: pinned
        width: 340
        cfgFlat: ({ "theme.source": "hybrid", "theme.custom.accent": "#83a598" })
        palettes: ({ "gruvbox": { label: "Gruvbox", accent: "#83a598" } })
        desktopTheme: "tokyo-night"
    }

    SignalSpy {
        id: syncSpy
        target: pinned
        signalName: "syncRequested"
    }

    // Bound is the quiet state: it states what colors follow, and offers no
    // action because there is nothing to undo.
    function test_bound_names_the_desktop_theme() {
        compare(bound.state_, "bound")
        verify(bound.summary.indexOf("tokyo-night") >= 0)
        compare(bound.canSync, false)
    }

    // Pinned is the loud state: it must name BOTH sides so the divergence is
    // legible, and offer the way back.
    function test_pinned_names_both_sides_and_offers_sync() {
        compare(pinned.state_, "pinned")
        verify(pinned.summary.indexOf("Gruvbox") >= 0)
        verify(pinned.summary.indexOf("tokyo-night") >= 0)
        compare(pinned.canSync, true)
    }

    function test_sync_emits_with_the_daemon_patch() {
        syncSpy.clear()
        pinned.requestSync()
        compare(syncSpy.count, 1)
        compare(JSON.stringify(pinned.syncPatch),
                JSON.stringify({ theme: { source: "omarchy" } }))
    }

    // Switching config re-derives without needing a reload.
    function test_state_is_reactive() {
        bound.cfgFlat = ({ "theme.source": "terminal" })
        compare(bound.state_, "index")
        bound.cfgFlat = ({ "theme.source": "omarchy" })
        compare(bound.state_, "bound")
    }
}
