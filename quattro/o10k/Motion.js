.pragma library

// Shared motion tokens for the Omarchy10k Control Center.
//
// Values are mirrored from the Omarchy Spatial UX plugin's lib/Motion.qml
// (~/syncthing/OMPSpacialUX). Both plugins load into the same omarchy-shell
// Quickshell process, so divergent timings read as two different apps
// sharing a desktop. Their singleton is plugin-private and cannot be
// imported, hence the mirror. tests/motion_test.js pins the values.
var MICRO_MS = 90;
var SHORT_MS = 140;
var MEDIUM_MS = 220;
var LONG_MS = 360;

// Every animation duration goes through scaled(), so a reduced-motion
// setting collapses the whole surface at once instead of relying on each
// call site to check.
function scaled(ms, speed) {
    var s = (speed === undefined || speed === null) ? 1 : Number(speed);
    if (!isFinite(s) || s < 0)
        s = 0;
    var v = Math.round(Number(ms) * s);
    return v > 0 ? v : 0;
}
