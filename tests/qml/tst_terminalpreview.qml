import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "TerminalPreview"

    readonly property var gruvbox: ({
        "background": "#282828", "foreground": "#ebdbb2", "accent": "#83a598",
        "red": "#fb4934", "green": "#b8bb26", "yellow": "#fabd2f",
        "blue": "#83a598", "magenta": "#d3869b", "cyan": "#8ec07c",
        "orange": "#fe8019", "muted": "#a89984"
    })

    readonly property var threeRows: [
        { label: "clean repo", left: "\x1b[32m~/x\x1b[0m", right: "" },
        { label: "command failed", left: "\x1b[31m127\x1b[0m", right: "1.2s" },
        { label: "over ssh", left: "ssh", right: "" }
    ]

    TerminalPreview { id: shown; width: 400; renders: parent.threeRows; colors: parent.gruvbox }
    TerminalPreview { id: loading; width: 400; renderState: "loading" }
    TerminalPreview { id: errored; width: 400; renderState: "error"; errorText: "unrepresentable patch" }
    TerminalPreview { id: noDaemon; width: 400; renderState: "empty" }
    TerminalPreview { id: idle; width: 400; renderState: "idle" }
    TerminalPreview { id: bare; width: 400; renders: parent.threeRows }

    // The central promise: the mock is drawn on the palette being PREVIEWED,
    // not on the Control Center's surface. A prompt swatched on the panel
    // background is a preview of the panel.
    function test_background_is_the_previewed_palettes_own() {
        compare(shown.bg.toString(), "#282828")
        compare(shown.fg.toString(), "#ebdbb2")
    }

    function test_falls_back_to_panel_colors_without_a_palette() {
        // Must not render transparent-on-transparent while the palette loads.
        verify(bare.bg.a === 1)
        verify(bare.fg.a === 1)
    }

    function test_rows_render_only_in_the_ok_state() {
        verify(shown.hasRows)
        verify(!loading.hasRows)
        verify(!errored.hasRows)
        verify(!noDaemon.hasRows)
        verify(!idle.hasRows)
    }

    // "nothing requested yet" and "no daemon" are different conditions and
    // must not share a message: the Studio sets idle on every tab switch, and
    // showing "No daemon" there accused a healthy setup of being broken.
    function test_idle_is_distinct_from_no_daemon() {
        verify(idle.renderState !== noDaemon.renderState)
    }

    function test_an_empty_render_list_is_not_ok_rows() {
        shown.renders = []
        verify(!shown.hasRows)
        shown.renders = threeRows
        verify(shown.hasRows)
    }

    function test_chrome_never_competes_with_the_prompt() {
        // Gutter ink is dimmed off the terminal's foreground, so it reads as
        // frame rather than as another prompt segment.
        verify(shown.chrome.a < shown.fg.a)
    }

    function test_survives_a_null_palette() {
        // Service.palettes is undefined until its first fetch lands.
        shown.colors = null
        verify(shown.bg.a === 1)
        shown.colors = gruvbox
    }

    function test_error_state_carries_the_daemons_message() {
        compare(errored.errorText, "unrepresentable patch")
        compare(errored.renderState, "error")
    }

    function test_column_count_is_stated() {
        // The render assumed a width; hiding that makes the preview dishonest.
        compare(shown.cols, 120)
    }
}
