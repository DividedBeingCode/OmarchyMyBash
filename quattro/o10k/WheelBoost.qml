import QtQuick

// Touchpad and wheel scrolling at a sane rate.
//
// Qt's default Flickable wheel handling is slow on a touchpad: a two-finger
// scroll arrives as a stream of small `pixelDelta` values with no
// acceleration, so a long pane takes an absurd amount of dragging. Panel.qml
// fixed this with a WheelHandler and a boost multiplier; the Studio was built
// later with plain Flickables and never got it, which is why the popout
// scrolls normally and the Studio does not.
//
// Extracted here so the two cannot drift again — pasting the block is how
// they diverged the first time.
//
// Usage, inside a Flickable:
//
//     Flickable {
//         id: scroll
//         WheelBoost { flick: scroll }
//     }
//
// NAMING: the multiplier must not share its name with this handler's id. In
// Panel.qml an `id: wheelBoost` beside a `property real wheelBoost` made every
// lookup resolve to the handler object, and the step computed NaN — a silent
// dead scroll. The property here is `boost`, and the id is deliberately
// different. The Flickable property is `flick`, NOT `target`: PointerHandler
// already defines `target`, and shadowing it is the same class of bug.
WheelHandler {
    id: wheel

    /// The Flickable to scroll. Required.
    property Flickable flick: null
    /// Multiplier applied to both the pixel and the notch path.
    property real boost: 3.0

    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    // Without this the handler stays "active" between gestures and swallows
    // the next one.
    activeTimeout: 0.5

    onWheel: (event) => {
        if (!wheel.flick) { event.accepted = false; return }

        const pd = event.pixelDelta
        const ady = event.angleDelta.y
        // Nothing usable in this event: let it through rather than eating it.
        if (!(pd && pd.y) && ady === 0) { event.accepted = false; return }

        const max = Math.max(0, wheel.flick.contentHeight - wheel.flick.height)
        // Touchpads report real pixels, so scale them directly. A mouse
        // reports notches of 120, each worth a quarter of the visible pane.
        const step = (pd && pd.y !== 0)
            ? -pd.y * wheel.boost
            : -(ady / 120) * wheel.flick.height * 0.25 * wheel.boost

        wheel.flick.contentY =
            Math.max(0, Math.min(max, wheel.flick.contentY + step))
        event.accepted = true
    }
}
