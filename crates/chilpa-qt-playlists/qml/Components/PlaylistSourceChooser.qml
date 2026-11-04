import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Where a new playlist is kept: in Chilpa, on a music server or at a source
// that edits playlists (`Playlists.sourcesJson`). Each is listed with its
// icon and a line under its name, what keeping it there does or why it
// cannot take these tracks, wrapped whole; one that cannot is greyed and is
// not taken. The line of the one chosen stays under the field.
Column {
    id: chooser
    property var sources: []
    property string sourceId: ""
    property string sourceName: ""
    signal picked(string id, string name)
    readonly property var rows: [{id: "", name: I18n.tr("Local", I18n.rev), local: true, accepts: true,
        detail: I18n.tr("Kept in Chilpa; tracks from any source mix in it", I18n.rev)}].concat(sources)
    readonly property var current: rows.find(row => row.id === sourceId) || rows[0]
    function line(row) {
        return row.accepts === true ? row.detail || "" : row.why || I18n.tr("Only tracks from this source go in its playlists", I18n.rev)
    }
    spacing: Metrics.gap6

    SelectField {
        id: field
        objectName: "playlistSourceChooser"
        width: parent.width
        rich: true
        value: chooser.current.id
        values: chooser.rows.map(row => row.id)
        options: chooser.rows.map(row => row.name)
        details: chooser.rows.map(row => chooser.line(row))
        enabledOptions: chooser.rows.map(row => row.accepts === true)
        sources: chooser.rows.map(row => row.local ? ({chilpa: true}) : ({icon: row.icon || ""}))
        onPicked: (value) => {
            const row = chooser.rows.find(one => one.id === value)
            if (row && row.accepts === true)
                chooser.picked(row.id, row.name)
        }
    }
    Text {
        objectName: "playlistSourceLine"
        width: parent.width
        visible: text !== ""
        text: chooser.line(chooser.current)
        wrapMode: Text.WordWrap
        textFormat: Text.PlainText
        font.pointSize: Type.small
        color: chooser.current.accepts === true ? Theme.color.textSecondary : Theme.color.statusWarning
    }
}
