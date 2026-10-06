import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "../../../chilpa-qt-kit/qml/Primitives"
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "PluginIcon"
    when: windowShown
    visible: true
    width: 960; height: 600

    NavItem { id: entry; width: 230; text: "Guest"; glyph: Icons.letterG }

    SourceIcon { id: badge; y: 80; width: 16; height: 16 }
    function test_tint_keeps_antialiased_edge_coverage_at_sidebar_size() {
        const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 192 192" fill="none"><circle cx="96" cy="96" r="76" stroke="#000" stroke-width="8"/><path d="M121 121L163 163" stroke="#000" stroke-linecap="round" stroke-width="12"/></svg>'
        entry.iconSource = "data:image/svg+xml," + encodeURIComponent(svg)
        entry.tintIcon = false
        const picture = findChild(entry, "navImage")
        tryCompare(picture, "status", Image.Ready)
        waitForRendering(entry)
        const original = grabImage(picture); entry.tintIcon = true
        waitForRendering(entry)
        const tinted = grabImage(picture); let edges = 0
        for (let y = 0; y < original.height; ++y) for (let x = 0; x < original.width; ++x) {
            verify(Math.abs(tinted.red(x,y) - (128 + original.red(x,y) * 127 / 255)) <= 1, "Tint must preserve coverage at " + x + "," + y)
            if (original.red(x,y) > 0 && original.red(x,y) < 255) ++edges
        }
        verify(edges > 0, "The fixture must include antialiased edges")
        entry.tintIcon = false; entry.iconSource = ""
    }
    function test_neutral_badges_follow_theme_without_disabled_opacity() {
        const svg = '<svg xmlns="http://www.w3.org/2000/svg"><rect width="16" height="16" fill="#000"/></svg>'
        badge.sourceInfo = {icon:"data:image/svg+xml," + encodeURIComponent(svg) + "#monochrome=1"}
        const picture = findChild(badge, "sourceImage")
        tryCompare(picture, "status", Image.Ready)
        for (const tone of ["#5c6673", "#abb5c3"]) {
            ThemeModel.palette = {"text.secondary":tone}; ThemeModel.rev += 1
            waitForRendering(badge)
            tryVerify(() => Qt.colorEqual(grabImage(badge).pixel(8,8), tone))
            compare(badge.opacity, 1)
        }
        ThemeModel.palette = {}; ThemeModel.rev += 1; badge.sourceInfo = {}
    }
    function test_tint_recolors_black_pixels_and_restores_the_original() {
        entry.iconSource = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg=="
        const picture = findChild(entry, "navImage")
        tryCompare(picture, "status", Image.Ready)
        const x = Math.round(picture.x + picture.width / 2)
        const y = Math.round(picture.y + picture.height / 2)
        entry.tintIcon = true
        tryCompare(picture, "color", Theme.color.accent)
        waitForRendering(entry)
        tryVerify(() => Qt.colorEqual(grabImage(entry).pixel(x, y), Theme.color.accent))
        entry.tintIcon = false; waitForRendering(entry)
        tryVerify(() => grabImage(entry).red(x, y) === 0)
        entry.iconSource = ""
    }

    function test_fallback_letter_exists_and_stays_visible_without_an_image() {
        entry.iconSource = ""
        verify(Icons.letterG.length > 0 && Icons.letterX.length > 0)
        tryCompare(findChild(entry, "navGlyph"), "visible", true)
        compare(findChild(entry, "navImage").visible, false)
        entry.collapsed = true
        compare(findChild(entry, "navGlyph").x, (entry.width - Metrics.iconNav) / 2)
        entry.collapsed = false
    }

    function test_a_valid_plugin_image_replaces_the_letter_and_removal_restores_it() {
        entry.iconSource = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg=="
        tryCompare(findChild(entry, "navImage"), "status", Image.Ready)
        compare(findChild(entry, "navGlyph").visible, false)
        entry.iconSource = ""
        tryCompare(findChild(entry, "navGlyph"), "visible", true)
    }
}
