import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Asked instead of adding when the chosen playlist already holds some of
// the tracks, as QBZ's: add only the new ones, or every one, repeats too.
Rectangle {
    id: question
    objectName: "playlistRepeats"

    property int repeated: 0
    property int trackCount: 0
    property bool busy: false
    signal answered(bool every)
    signal cancelled()

    color: Theme.color.surfaceRaised
    radius: Metrics.radiusControl

    // The picker underneath takes no click while this is asked.
    MouseArea { anchors.fill: parent }

    Column {
        anchors.fill: parent
        spacing: Metrics.gap16
        Row {
            width: parent.width
            Text {
                width: parent.width - close.width
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Some of these tracks were already added", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            IconButton { id: close; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: question.cancelled() }
        }
        Text {
            width: parent.width
            text: I18n.tr("This playlist already contains {} of the {} tracks you're adding. Add only the new ones, or add everything including duplicates.", I18n.rev)
                  .replace("{}", question.repeated).replace("{}", question.trackCount)
            wrapMode: Text.Wrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap12
            PillButton {
                objectName: "addEveryTrack"
                enabled: !question.busy
                text: question.busy ? I18n.tr("Adding...", I18n.rev) : I18n.tr("Add all tracks", I18n.rev)
                onClicked: question.answered(true)
            }
            PillButton {
                objectName: "addNewTracks"
                kind: "primary"
                enabled: !question.busy
                text: question.busy ? I18n.tr("Adding...", I18n.rev) : I18n.tr("Add only new tracks", I18n.rev)
                onClicked: question.answered(false)
            }
        }
    }
}
