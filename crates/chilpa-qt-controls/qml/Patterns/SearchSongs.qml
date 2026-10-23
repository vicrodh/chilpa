import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// The songs the results page lists, one under the other. While songs are
// being chosen each row wears its check, and a click chooses it, or with
// Shift every one from the last clicked; its menu stays on the right button.
Column {
    id: songs

    property var rows: []
    // What keeps the songs chosen, where they can be chosen.
    property var chosen: null
    // A cover's address, as the page writes it.
    property var artUrl: art => art
    signal activated(int at)
    signal played(int at)
    signal menuAsked(int at, Item item, real left, real top)

    Repeater {
        model: songs.rows
        Item {
            id: line
            required property var modelData
            required property int index
            readonly property bool checking: songs.chosen !== null && songs.chosen.selecting
            readonly property bool picked: checking && songs.chosen.picked[modelData.key] === true
            width: songs.width; height: hit.height
            Icon {
                id: check
                visible: line.checking
                x: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                glyph: line.picked ? Icons.squareCheckFilled : Icons.square
                filled: line.picked
                size: Metrics.iconMd
                color: line.picked ? Theme.color.accent : Theme.color.textTertiary
            }
            SearchHit {
                id: hit
                x: line.checking ? check.x + check.width + Metrics.gap4 : 0 // derived: after the check
                width: parent.width - x
                current: line.picked
                index: line.modelData.at; kind: line.modelData.kind
                title: line.modelData.title; detail: line.modelData.detail
                art: songs.artUrl(line.modelData.art); duration: line.modelData.duration
                sourceIcon: line.modelData.sourceIcon; sourceName: line.modelData.sourceName
                markOwn: true
                onActivated: songs.activated(index)
                onPlayed: songs.played(index)
                onMenu: (left, top) => songs.menuAsked(index, hit, left, top)
            }
            MouseArea {
                objectName: "searchSongChoice"
                anchors.fill: parent
                visible: line.checking
                cursorShape: Qt.PointingHandCursor
                onClicked: (mouse) => songs.chosen.choose(line.index, (mouse.modifiers & Qt.ShiftModifier) !== 0)
            }
        }
    }
}
