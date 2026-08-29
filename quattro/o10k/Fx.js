.pragma library

// Shared surface effects for the Omarchy10k Control Center.
//
// This library owns ONLY what omarchy-shell's Style singleton does not
// provide: a corner-radius floor and elevation parameters. Interactive
// state colors are NOT here — Style already exposes normalFill / hoverFill /
// selectedFill / pressedFill / focusFill, which themes can override via
// Style.styleOverrides. Defining parallel alphas would make our controls
// un-themeable and visually foreign.

// Style.cornerRadius mirrors Hyprland's decoration:rounding, which Omarchy
// ships at 0. Honoring it faithfully renders every surface as a hard
// rectangle, so the kit floors it. A theme asking for MORE rounding keeps
// its value — this is a minimum, not an override.
var RADIUS_FLOOR = 8;

function radius(styleCornerRadius) {
    var r = Number(styleCornerRadius);
    if (!isFinite(r) || r < 0)
        r = 0;
    return Math.max(r, RADIUS_FLOOR);
}

// Elevation levels expressed as RectangularShadow parameters.
//
// RectangularShadow computes a rounded-rect falloff analytically in ONE
// quad. MultiEffect/DropShadow instead require layer.enabled on the
// shadowed item — an offscreen buffer per surface — plus a multi-tap blur.
// On the integrated-GPU target shared with the Spatial UX plugin that is
// the difference between free and not, so consumers must use
// RectangularShadow exclusively.
var ELEVATION = {
    flat:   { blur: 0,  spread: 0, offsetY: 0, opacity: 0.0 },
    rest:   { blur: 12, spread: 0, offsetY: 2, opacity: 0.18 },
    raised: { blur: 24, spread: 0, offsetY: 6, opacity: 0.28 }
};

function elevation(level, shadowsEnabled) {
    if (shadowsEnabled === false)
        return ELEVATION.flat;
    var e = ELEVATION[level];
    return e ? e : ELEVATION.flat;
}
