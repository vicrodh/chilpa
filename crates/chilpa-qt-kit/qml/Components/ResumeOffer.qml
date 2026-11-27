import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// Play on an album or a playlist left halfway, with "Remember position in
// playlists and albums" on: go on from the last track that began, or start
// over. The facade leaves the offer (`QueueActivity.offer`) instead of
// playing; Esc or a click outside plays nothing.
Modal {
    id: dialog
    // The offer as JSON; the Gallery hands one and opens it itself.
    property string source: QueueActivity.offer
    property bool automatic: true
    readonly property var offer: JSON.parse(source || "{}")
    property bool answered: false

    preferredWidth: Metrics.confirmWidth
    onOfferChanged: if (automatic && offer.title !== undefined && !opened) { answered = false; open() }
    onClosed: if (!answered) QueueActivity.answer("")

    function answer(choice) {
        answered = true
        close()
        QueueActivity.answer(choice)
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Go on where you left off?", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "resumeOfferText"
            width: parent.width
            text: I18n.trArgs("The last track that played here was “{}”, {} of {}.", [dialog.offer.title || "", Number(dialog.offer.number || 0).toLocaleString(Qt.locale(), 'f', 0), Number(dialog.offer.total || 0).toLocaleString(Qt.locale(), 'f', 0)], I18n.rev)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton {
                objectName: "resumeRestart"
                text: I18n.tr("Start over", I18n.rev)
                onClicked: dialog.answer("restart")
            }
            PillButton {
                objectName: "resumeContinue"
                kind: "primary"
                text: I18n.tr("Continue", I18n.rev)
                onClicked: dialog.answer("continue")
            }
        }
    }
}
