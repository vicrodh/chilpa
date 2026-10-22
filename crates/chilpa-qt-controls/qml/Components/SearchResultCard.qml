import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The top result of the results page, at the library's card size: its
// cover, round for an artist, what it is called and what it is, with the
// icon of the source it comes from or Chilpa's mark for the library's own.
Item {
    id: card
    property var row: ({})
    readonly property bool person: row.kind === 1 || row.kind === 2
    signal activated()
    signal menuAsked(real x, real y)
    width: CardLayout.cardWidth
    height: CardLayout.cardHeight
    Accessible.role: Accessible.Button
    Accessible.name: row.title || ""

    ArtTile {
        id: cover
        width: parent.width; height: width
        radius: card.person ? width / 2 : Metrics.radiusCard // derived: a round portrait
        path: (card.row.art || "").startsWith("https://") ? "" : card.row.art || ""
        source: (card.row.art || "").startsWith("https://") ? card.row.art : path === "" ? "" : "file://" + path.split("/").map(encodeURIComponent).join("/")
    }
    // Whose it is: the source's icon, or Chilpa's mark for the library's own.
    Rectangle {
        objectName: "resultOrigin"
        anchors.right: cover.right; anchors.bottom: cover.bottom; anchors.margins: Metrics.overlayPad
        width: Metrics.iconSm + Metrics.gap8; height: width // derived: the icon and a ring around it
        radius: width / 2 // derived: a disc
        color: Theme.color.overlayScrim
        SourceIcon {
            anchors.centerIn: parent
            width: Metrics.iconSm; height: Metrics.iconSm
            sourceInfo: (card.row.sourceIcon || "") !== "" ? ({icon: card.row.sourceIcon}) : ({chilpa: true})
            color: Theme.color.backdropText
        }
    }
    Text {
        anchors.top: cover.bottom; anchors.topMargin: Metrics.cardTextGap
        width: parent.width
        text: card.row.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight
        horizontalAlignment: card.person ? Text.AlignHCenter : Text.AlignLeft
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }
    Text {
        anchors.bottom: parent.bottom
        width: parent.width
        visible: !card.person
        text: card.row.detail || ""; textFormat: Text.PlainText; elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => button === Qt.RightButton ? card.menuAsked(point.position.x, point.position.y) : card.activated()
    }
}
