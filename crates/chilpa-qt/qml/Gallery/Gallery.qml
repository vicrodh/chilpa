import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Patterns"

// Every primitive, component and pattern, in its states, in the theme
// chosen at the top: the registry of design/03 made visible. A piece that
// is not here does not exist yet (tools/lint/qml-imports checks it).
// Opened with CHILPA_GALLERY=1.
Window {
    id: window

    width: Metrics.windowWidth
    height: Metrics.windowHeight
    visible: true
    color: Theme.color.surfaceWindow
    title: "Chilpa Gallery"

    SegmentedControl {
        id: modes
        x: Metrics.contentX
        y: Metrics.contentTop
        options: [
            { glyph: Icons.square, tip: I18n.tr("Light", I18n.rev) },
            { glyph: Icons.circle, tip: I18n.tr("Dark", I18n.rev) }
        ]
        current: Theme.dark ? 1 : 0
        onActivated: (index) => ThemeModel.mode = index === 1 ? "dark" : "light"
    }

    Flickable {
        anchors.fill: parent
        anchors.topMargin: Metrics.tabBarHeight
        contentHeight: sections.height + Metrics.contentBottom
        clip: true
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: sections
            x: Metrics.contentX
            width: window.width - Metrics.contentX * 2 // derived: padding on both sides
            spacing: Metrics.section

            GalleryPrimitives { width: parent.width }
            GalleryComponents { width: parent.width }
            GalleryPatterns { width: parent.width }
        }
    }

    Tooltip {}

    DevHooks {
        window: window
    }
}
