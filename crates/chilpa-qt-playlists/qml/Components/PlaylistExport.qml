import QtQuick
import QtQuick.Dialogs
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/LocalPaths.js" as LocalPaths
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Saves a playlist as a file another player opens: M3U8 with the tracks
// that are files here, XSPF or the importer's JSON with every track, the
// others by name. Once it is written it says how it went.
Item {
    id: exporter
    property int playlistId: 0
    // What writing the file left: written, left_out and error.
    property var result: ({})

    function show(id) {
        playlistId = id
        picker.open()
    }
    function save(path) {
        result = JSON.parse(PlaylistManager.exportTo(playlistId, path) || "{}")
        outcome.open()
    }

    FileDialog {
        id: picker
        objectName: "playlistExportPicker"
        fileMode: FileDialog.SaveFile
        title: I18n.tr("Export playlist", I18n.rev)
        nameFilters: [I18n.tr("M3U8 playlist", I18n.rev) + " (*.m3u8)", I18n.tr("XSPF playlist", I18n.rev) + " (*.xspf)",
            I18n.tr("JSON playlist", I18n.rev) + " (*.json)"]
        defaultSuffix: ["m3u8", "xspf", "json"][selectedNameFilter.index] || "m3u8"
        onAccepted: exporter.save(LocalPaths.fromUrl(selectedFile))
    }

    Modal {
        id: outcome
        objectName: "playlistExportOutcome"
        Column {
            width: parent.width
            spacing: Metrics.gap12
            Text {
                width: parent.width
                text: (exporter.result.error || "") !== "" ? I18n.tr("The playlist could not be exported", I18n.rev) : I18n.tr("Playlist exported", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Text {
                objectName: "playlistExportMessage"
                width: parent.width
                text: (exporter.result.error || "") !== "" ? exporter.result.error
                    : I18n.trCount("{} track was written.", "{} tracks were written.", exporter.result.written || 0, I18n.rev)
                    + ((exporter.result.left_out || 0) > 0 ? " " + I18n.trCount("{} track is not a file on this computer and was left out; an XSPF or JSON file keeps it by name.",
                        "{} tracks are not files on this computer and were left out; an XSPF or JSON file keeps them by name.", exporter.result.left_out, I18n.rev) : "")
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
}
