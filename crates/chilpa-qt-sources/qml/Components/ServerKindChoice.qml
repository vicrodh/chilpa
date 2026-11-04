import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The first step of adding a server: which kind it is, each with its
// source icon, its name and what it is in one sentence.
Column {
    id: kinds
    property string chosen: ""
    signal picked(string kind)
    // The products are named as they name themselves, in every language.
    readonly property var offered: [
        {id: "jellyfin", icon: "jellyfin", name: "Jellyfin",
            about: I18n.tr("A media server you run yourself. You sign in with its user and password, or with Quick Connect.", I18n.rev)},
        {id: "subsonic", icon: "navidrome", name: "Navidrome / OpenSubsonic",
            about: I18n.tr("Navidrome, gonic or another server that speaks Subsonic. You sign in with its user and password, or an API key.", I18n.rev)},
        {id: "plex", icon: "plex", name: "Plex",
            about: I18n.tr("A Plex Media Server. You sign in with your Plex account, with a code at plex.tv/link.", I18n.rev)},
        {id: "dlna", icon: "dlna", name: "DLNA / UPnP",
            about: I18n.tr("A media server on this network, such as MiniDLNA, Gerbera or the DLNA side of Plex. There is nothing to sign in to.", I18n.rev)}
    ]
    spacing: Metrics.gap8

    Repeater {
        model: kinds.offered
        Rectangle {
            id: card
            required property var modelData
            objectName: "serverKind_" + modelData.id
            readonly property bool selected: kinds.chosen === modelData.id
            width: kinds.width
            height: Math.max(Metrics.settingsRow, words.implicitHeight + Metrics.gap12 * 2) // derived: padding above and below
            radius: Metrics.radiusCard
            color: selected ? Theme.color.rowSelected : area.containsMouse ? Theme.color.stateHover : Theme.color.fillField
            border.width: selected ? Metrics.focusRing : 0
            border.color: Theme.color.accent

            SourceIcon {
                id: glyph
                objectName: "serverKindIcon"
                x: Metrics.gap12
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.iconPlay
                height: width
                sourceInfo: ({icon: card.modelData.icon})
                color: card.selected ? Theme.color.accent : Theme.color.textSecondary
            }
            Column {
                id: words
                anchors.left: glyph.right
                anchors.leftMargin: Metrics.gap12
                anchors.right: mark.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap2
                Text {
                    width: parent.width
                    text: card.modelData.name
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    font.pointSize: Type.body
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                Text {
                    width: parent.width
                    text: card.modelData.about
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.meta
                    color: Theme.color.textSecondary
                }
            }
            Icon {
                id: mark
                anchors.right: parent.right
                anchors.rightMargin: Metrics.gap12
                anchors.verticalCenter: parent.verticalCenter
                glyph: card.selected ? Icons.circleCheckFilled : Icons.circle
                filled: card.selected
                size: Metrics.iconMd
                color: card.selected ? Theme.color.accent : Theme.color.textTertiary
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: kinds.picked(card.modelData.id)
            }
            // Tab reaches each kind, and Space or Enter chooses it.
            activeFocusOnTab: true
            Keys.onPressed: (event) => {
                if (event.key !== Qt.Key_Space && event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
                kinds.picked(card.modelData.id)
                event.accepted = true
            }
            FocusRing {}
        }
    }
}
