import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What is known of the track that plays, opened from its title in the
// player: the same answer as the queue's Info tab, with the file or server
// entry it comes from.
Modal {
    id: modal
    objectName: "trackInfoModal"
    preferredWidth: Metrics.trackInfoWidth + Metrics.modalInsets
    preferredHeight: Metrics.trackInfoHeight + Metrics.modalInsets

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Row {
            id: header
            width: parent.width
            Text {
                objectName: "trackInfoTitle"
                width: parent.width - closeButton.width
                anchors.verticalCenter: parent.verticalCenter
                text: Player.title
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: modal.close() }
        }
        QueueAbout {
            objectName: "trackInfoAbout"
            width: parent.width
            height: Math.max(0, parent.parent.height - header.height - Metrics.gap12)
        }
    }
}
