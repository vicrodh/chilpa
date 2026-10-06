import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
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
        id: view
        anchors.fill: parent
        anchors.topMargin: Metrics.tabBarHeight
        contentHeight: sections.height + Metrics.contentBottom
        clip: true
        ScrollBar.vertical: ViewportScrollBar {}

        Column {
            id: sections
            x: Metrics.contentX
            width: window.width - Metrics.contentX * 2 // derived: padding on both sides
            spacing: Metrics.section

            GalleryPrimitives { width: parent.width }
            GalleryComponents { width: parent.width }
            GalleryPatterns { id: patterns; width: parent.width }
            Loader { width: parent.width; source: "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Gallery/GalleryMetadata.qml" }
            Loader { width: parent.width; source: "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Gallery/GalleryPlugins.qml" }
            Loader { width: parent.width; source: "qrc:/qt/qml/com/blitzfc/chilpa/controls/qml/Gallery/GalleryControls.qml" }
            Loader { width: parent.width; source: "qrc:/qt/qml/com/blitzfc/chilpa/lyrics/qml/Gallery/GalleryLyrics.qml" }
        }
    }

    Tooltip {}

    Rectangle {
        anchors.fill: parent
        visible: Dev.route === "quality"
        z: Metrics.tooltipLayer
        color: Theme.color.surfaceWindow

        Column {
            anchors.centerIn: parent
            spacing: Metrics.gap10

            QualityTooltip { anchors.horizontalCenter: parent.horizontalCenter }
            PlayerBar { width: Math.min(window.width - Metrics.contentX * 2, Metrics.playerMaxWidth) } // derived: inset on each side
        }
    }

    Rectangle {
        anchors.fill: parent
        visible: Dev.route === "menu"
        z: Metrics.tooltipLayer
        color: Theme.color.surfaceWindow
    }

    Menu {
        id: designMenu
        x: (window.width - width) / 2 // derived: centered in the gallery
        y: (window.height - height) / 2 // derived: centered in the gallery
        title: "A 25 años"
        entries: [
            { text: "Play now", glyph: Icons.playerPlayFilled, filled: true },
            { text: "Play next" },
            { text: "Add to queue" },
            { text: "Add to playlist…", submenu: true },
            { separator: true },
            { text: "Open album" }
        ]
    }

    DevHooks {
        window: window
    }

    Component.onCompleted: {
        if (Dev.route === "patterns")
            Qt.callLater(() => view.contentY = patterns.y + Dev.routeId)
        else if (Dev.route === "menu")
            Qt.callLater(designMenu.open)
    }
}
