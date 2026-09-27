import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// What the metadata editor edits: the record's fields, then its tracks --
// disc, number, title and artist in place -- and every field of the track
// chosen. A change goes to the facade as soon as a field is left. `doc` is
// the editor's view.
Column {
    id: root

    property var doc: ({})
    // The track whose every field shows. It starts where the editor was
    // opened from.
    property int chosen: 0

    readonly property var tracks: doc.tracks || []
    readonly property var track: chosen >= 0 && chosen < tracks.length ? tracks[chosen] : null

    spacing: Metrics.gap8

    Connections {
        target: MetadataEditor
        function onOpenChanged() {
            if (MetadataEditor.open)
                root.chosen = Math.max(0, root.doc.focus || 0)
        }
    }

    Text {
        text: I18n.tr("Record", I18n.rev)
        font.pointSize: Type.section
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    Repeater {
        // By count, not by list: a new view does not rebuild the fields, so
        // the one being typed in keeps the keyboard.
        model: (root.doc.album || []).length
        MetadataField {
            required property int index
            width: root.width
            field: root.doc.album[index]
            onEdited: (name, value) => MetadataEditor.setAlbum(name, value)
        }
    }

    Item { width: 1; height: Metrics.gap12 }
    Text {
        text: I18n.tr("Tracks", I18n.rev)
        font.pointSize: Type.section
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    Repeater {
        model: root.tracks.length
        MetadataTrackRow {
            required property int index
            width: root.width
            rowIndex: index
            track: root.tracks[index]
            chosen: index === root.chosen
            onChoose: root.chosen = index
            onEdited: (trackId, name, value) => MetadataEditor.setTrack(trackId, name, value)
        }
    }

    Item { width: 1; height: Metrics.gap12; visible: root.track !== null }
    Text {
        visible: root.track !== null
        width: root.width
        text: root.track ? I18n.trArgs("Track: {}", [root.track.file], I18n.rev) : ""
        elide: Text.ElideMiddle
        font.pointSize: Type.section
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    Repeater {
        model: root.track ? root.track.fields.length : 0
        MetadataField {
            required property int index
            width: root.width
            ofTrack: true
            field: root.track ? root.track.fields[index] : ({})
            onEdited: (name, value) => MetadataEditor.setTrack(root.track.id, name, value)
        }
    }
}
