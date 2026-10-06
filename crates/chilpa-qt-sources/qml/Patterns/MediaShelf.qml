import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

Column {
    id: shelf
    property string title: ""
    property var entries: []
    property bool station: false
    signal opened(var entry)
    signal played(var entry)
    signal saved(var entry)
    signal more()
    property int shown: 20
    spacing: Metrics.gap16
    Item {
        width: parent.width; height: Metrics.button32
        Text { anchors.left: parent.left; anchors.right: more.left; anchors.verticalCenter: parent.verticalCenter; text: shelf.title; font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.textPrimary; elide: Text.ElideRight }
        PillButton { id: more; anchors.right: parent.right; visible: shelf.entries.length > shelf.shown; text: I18n.tr("Load more", I18n.rev); kind: "ghost"; onClicked: shelf.shown += 20 }
    }
    MediaGrid { width: parent.width; entries: shelf.entries.slice(0, shelf.shown); station: shelf.station; onOpened: entry => shelf.opened(entry); onPlayed: entry => shelf.played(entry); onSaved: entry => shelf.saved(entry) }
}
