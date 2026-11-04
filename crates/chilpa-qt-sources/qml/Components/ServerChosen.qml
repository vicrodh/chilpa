import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What comes in from a media server, kept in sight under its folders while
// they are browsed (ServerWizard): the folders chosen at any level, by the
// way down to each, with a cross to take one out. Past a few, the list
// scrolls in its own place and the folders above keep theirs.
Column {
    id: tray
    objectName: "serverChosen"
    property var chosen: []
    property var names: ({})
    // How many rows show before the list scrolls.
    property int rows: 3
    signal picked(string id, string name, bool on)
    spacing: Metrics.gap6

    Rectangle {
        width: parent.width
        height: Metrics.hairline
        color: Theme.color.strokeHairline
    }
    Text {
        objectName: "serverChosenTitle"
        width: parent.width
        text: tray.chosen.length === 0 ? I18n.tr("Nothing chosen yet. Choose at least one folder.", I18n.rev)
            : I18n.trCount("{} folder comes in:", "{} folders come in:", tray.chosen.length, I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
    ListView {
        id: list
        objectName: "serverChosenList"
        width: parent.width
        height: Math.min(contentHeight, Metrics.button24 * tray.rows + spacing * (tray.rows - 1)) // derived: `rows` rows, then it scrolls
        visible: count > 0
        clip: true
        spacing: Metrics.gap4
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        model: tray.chosen
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap4; policy: ScrollBar.AlwaysOn }
        delegate: Row {
            id: kept
            required property string modelData
            required property int index
            width: list.width - Metrics.gap12
            spacing: Metrics.gap6
            IconButton {
                id: drop
                objectName: "serverDropFolder_" + kept.index
                size: Metrics.button24
                glyph: Icons.x
                tip: I18n.tr("Take it out", I18n.rev)
                onClicked: tray.picked(kept.modelData, "", false)
            }
            Text {
                width: parent.width - drop.width - parent.spacing
                anchors.verticalCenter: parent.verticalCenter
                text: tray.names[kept.modelData] || I18n.tr("A folder chosen before", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
        }
    }
}
