import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

ListView {
    id: found
    property string query: ""
    property int current: 0
    readonly property var matches: {
        try {
            return JSON.parse(SettingsModel.find(found.query, I18n.rev))
        } catch (e) {
            return []
        }
    }
    signal chosen(var setting)
    function step(by) {
        current = Math.max(0, Math.min(matches.length - 1, current + by))
        positionViewAtIndex(current, ListView.Contain)
    }
    function choose() {
        if (matches.length > 0)
            chosen(matches[current])
    }
    onMatchesChanged: current = 0
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    model: matches
    spacing: Metrics.gap2
    ScrollBar.vertical: ViewportScrollBar {}
    delegate: Rectangle {
        required property var modelData
        required property int index
        objectName: "settingFound"
        width: ListView.view.width - Metrics.gap12
        height: lines.implicitHeight + Metrics.gap12 * 2 // derived: padding above and below
        radius: Metrics.radiusControl
        color: index === found.current || hover.hovered ? Theme.color.stateHover : "transparent"
        Column {
            id: lines
            x: Metrics.gap12
            y: Metrics.gap12
            width: parent.width - Metrics.gap12 * 2 // derived: padding on both sides
            spacing: Metrics.gap2
            Text { width: parent.width; text: modelData.title; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
            Text { width: parent.width; text: modelData.place; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textTertiary }
            Text { width: parent.width; visible: text !== ""; text: modelData.description; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }
        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: found.chosen(modelData) }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: found.count === 0
        text: I18n.tr("No setting says that", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}
