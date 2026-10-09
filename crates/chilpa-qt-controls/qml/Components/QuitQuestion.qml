import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The question before quitting, as QBZ asks it, with a way to stop asking.
// Cancel, Esc or a click outside keeps Chilpa open.
Modal {
    id: question
    signal quit(bool stopAsking)
    function ask() {
        stopAsking.checked = false
        open()
    }
    width: Metrics.confirmWidth

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Quit Chilpa?", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: I18n.tr("Playback stops and the window closes.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        CircleCheck { id: stopAsking; objectName: "stopAskingQuit"; text: I18n.tr("Don't ask again", I18n.rev) }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { objectName: "keepOpen"; text: I18n.tr("Cancel", I18n.rev); onClicked: question.close() }
            PillButton {
                objectName: "quitNow"
                kind: "primary"
                text: I18n.tr("Quit", I18n.rev)
                onClicked: {
                    question.close()
                    question.quit(stopAsking.checked)
                }
            }
        }
    }
}
