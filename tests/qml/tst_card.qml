import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "Card"

    Card {
        id: restCard
        width: 200; height: 80
    }

    Card {
        id: flatCard
        width: 200; height: 80
        elevation: "flat"
    }

    Card {
        id: noShadowCard
        width: 200; height: 80
        elevation: "raised"
        shadowsEnabled: false
    }

    // Stock Omarchy has Style.cornerRadius = 0; the card must still be
    // rounded or every Control Center surface reads as a hard rectangle.
    function test_radius_is_floored_on_stock_omarchy() {
        compare(restCard.radius, 8)
    }

    function test_default_elevation_is_rest() {
        compare(restCard.elevation, "rest")
        verify(restCard.shadowOpacity > 0)
    }

    function test_flat_elevation_draws_no_shadow() {
        compare(flatCard.shadowOpacity, 0)
    }

    // Regression: Style.normalFill is a 4-8% alpha TINT, not a surface
    // colour. Using one as the base rendered a ~96% transparent card with
    // the desktop wallpaper showing straight through it.
    function test_surface_is_opaque() {
        compare(restCard.color.a, 1.0)
    }

    // The accessibility escape hatch must actually remove the shadow.
    function test_shadows_disabled_draws_no_shadow() {
        compare(noShadowCard.shadowOpacity, 0)
    }
}
