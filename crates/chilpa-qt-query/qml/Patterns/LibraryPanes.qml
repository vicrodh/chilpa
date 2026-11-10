import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// The Library's filter columns side by side, in the design's proportions,
// with a hairline between them (design/02 9).
Rectangle {
    id: panes

    radius: Metrics.radiusMenu
    color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfacePane
    clip: true

    // Each filter column can be turned on or off independently.
    property var shown: ["year", "genre", "artist", "album"]
    readonly property var all: [
        { column: "year", title: I18n.tr("Year", I18n.rev) },
        { column: "genre", title: I18n.tr("Genre", I18n.rev) },
        { column: "label", title: I18n.tr("Label", I18n.rev) },
        { column: "composer", title: I18n.tr("Composer", I18n.rev) },
        { column: "rating", title: I18n.tr("Rating", I18n.rev) },
        { column: "bits", title: I18n.tr("Bit depth", I18n.rev) },
        { column: "rate", title: I18n.tr("Sample rate", I18n.rev) },
        { column: "artist", title: I18n.tr("Artist", I18n.rev) },
        { column: "album", title: I18n.tr("Album", I18n.rev) }
    ]
    property var columns: []
    property bool built: false
    readonly property real shares: columns.reduce((sum, c) => sum + Metrics.paneShare[c.column], 0)
    PageLifetime { id: lifetime }
    function rebuild() {
        const selected = all.filter(c => shown.indexOf(c.column) >= 0)
        if (columns.length === selected.length && columns.every((entry, index) =>
                entry.column === selected[index].column && entry.title === selected[index].title)) return
        // Cancel cached row creation while each column context is still alive.
        for (let index = 0; index < repeated.count; ++index)
            lifetime.cancel(repeated.itemAt(index))
        columns = selected
    }
    onShownChanged: if (built) rebuild()
    onAllChanged: if (built) rebuild()
    Component.onCompleted: { built = true; rebuild() }

    Row {
        id: group
        anchors.fill: parent
        anchors.margins: 1
        spacing: 1
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { maskEnabled: true; maskSource: groupMask }

        Repeater {
            id: repeated
            model: panes.columns

            PaneColumn {
                required property int index
                required property var modelData
                width: (panes.width - panes.columns.length - 1) * Metrics.paneShare[modelData.column] / panes.shares // derived: the seams between columns and at the edges
                height: parent.height
                column: modelData.column
                title: modelData.title
                Rectangle {
                    anchors.right: parent.right
                    width: 1; height: parent.height
                    visible: index < panes.columns.length - 1
                    color: Theme.color.strokeHairline
                }
            }
        }
    }
    Rectangle {
        id: groupMask
        width: group.width; height: group.height
        radius: panes.radius
        color: Theme.color.accent
        layer.enabled: true
        visible: false
    }
    Edge {}
}
