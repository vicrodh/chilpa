import QtQuick
import QtQuick.Window
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The songs chosen on the results page and the bar of what can be done
// with them, as the Tracks screen's: play, queue, play next or later, add
// to a playlist, and the way out. A song is kept by its key, so results
// that arrive from another source do not move the choice; what is acted on
// is what is chosen among the rows shown.
Item {
    id: chosen

    property SearchModel search
    // The songs that can be chosen, as the page shows them.
    property var rows: []
    // The page is shown on a tab of songs: the keys only work there.
    property bool active: false
    property bool selecting: false
    property var picked: ({})
    // The last row clicked, where a range starts.
    property int anchor: -1
    readonly property var keys: rows.filter(row => picked[row.key] === true).map(row => row.key)

    function choose(at, range) {
        const next = Object.assign({}, picked)
        if (range && anchor >= 0) {
            for (let i = Math.min(anchor, at); i <= Math.max(anchor, at); i++) if (rows[i]) next[rows[i].key] = true
        } else {
            const key = rows[at].key
            if (next[key]) delete next[key]
            else next[key] = true
            anchor = at
        }
        picked = next
    }
    function chooseAll() {
        const next = {}
        rows.forEach(row => next[row.key] = true)
        selecting = true
        picked = next
    }
    function stop() { selecting = false; picked = ({}); anchor = -1 }
    function act(what) { search.actSongs(JSON.stringify(keys), what, I18n.tr("Search", I18n.rev)) }

    implicitWidth: actions.implicitWidth + Metrics.selectionHorizontalPadding
    implicitHeight: Metrics.selectionBarHeight

    // Escape lets the choice go; Ctrl+A takes every song shown.
    Shortcut { sequence: "Escape"; enabled: !InputLayers.blocksShortcuts(chosen.Window.window) && (chosen.active && chosen.selecting); onActivated: chosen.stop() }
    Shortcut { sequence: StandardKey.SelectAll; enabled: !InputLayers.blocksShortcuts(chosen.Window.window) && (chosen.active && chosen.rows.length > 0); onActivated: chosen.chooseAll() }

    Item {
        objectName: "searchChosenBar"
        anchors.fill: parent
        visible: chosen.selecting && chosen.keys.length > 0
        // Clicks in its empty space do not reach the rows under it.
        // no cursor: keeps clicks off the list beneath
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusPanel
            color: Theme.color.surfaceRaised
            Edge {}
        }
        Row {
            id: actions
            anchors.centerIn: parent
            spacing: Metrics.gap6
            Text {
                objectName: "searchChosenCount"
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: Metrics.gap10
                text: I18n.trCount("{} track", "{} tracks", chosen.keys.length, I18n.rev)
                font.pointSize: Type.body
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            IconButton { glyph: Icons.playerPlayFilled; filled: true; tone: "accent"; tip: I18n.tr("Play", I18n.rev); onClicked: chosen.act("play") }
            IconButton { objectName: "searchChosenQueue"; glyph: Lucide.listEnd; iconFamily: Lucide.font; tip: I18n.tr("Add to queue", I18n.rev); onClicked: chosen.act("queue") }
            IconButton { glyph: Lucide.listStart; iconFamily: Lucide.font; tip: I18n.tr("Play next", I18n.rev); onClicked: chosen.act("next") }
            IconButton { glyph: Lucide.listPlus; iconFamily: Lucide.font; tip: I18n.tr("Play later", I18n.rev); onClicked: chosen.act("later") }
            IconButton { objectName: "searchChosenPlaylist"; glyph: Icons.playlistAdd; tip: I18n.tr("Add to playlist…", I18n.rev); onClicked: chosen.search.pickSongs(JSON.stringify(chosen.keys)) }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 1; height: Metrics.iconNav; color: Theme.color.strokeHairline }
            IconButton { glyph: Icons.x; tip: I18n.tr("Finish selecting", I18n.rev); onClicked: chosen.stop() }
        }
    }
}
