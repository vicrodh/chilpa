import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What exporting a list left, once it is written: how many tracks went in
// and how many had no file here, or why it could not be written. Where it
// went was asked with the system's save dialog (`PlaylistManager.exportList`).
// The window holds one.
Modal {
    id: outcome
    objectName: "playlistExportOutcome"
    // What writing the file left: written, left_out and error.
    property var result: ({})

    Connections {
        target: PlaylistManager
        function onExported(result) {
            outcome.result = JSON.parse(result || "{}")
            outcome.open()
        }
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: (outcome.result.error || "") !== "" ? I18n.tr("The playlist could not be exported", I18n.rev) : I18n.tr("Playlist exported", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "playlistExportMessage"
            width: parent.width
            text: (outcome.result.error || "") !== "" ? outcome.result.error
                : I18n.trCount("{} track was written.", "{} tracks were written.", outcome.result.written || 0, I18n.rev)
                + ((outcome.result.left_out || 0) > 0 ? " " + I18n.trCount("{} track is not a file on this computer and was left out; an XSPF or JSON file keeps it by name.",
                    "{} tracks are not files on this computer and were left out; an XSPF or JSON file keeps them by name.", outcome.result.left_out, I18n.rev) : "")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        PillButton {
            anchors.right: parent.right
            kind: "primary"
            text: I18n.tr("Close", I18n.rev)
            onClicked: outcome.close()
        }
    }
}
