import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// One field of the metadata editor: its name above what it holds, and what
// the facade says about it -- the tracks disagree ("mixed"), it was changed,
// a sidecar lays it over the file, or it cannot be saved as it is. As a
// `cell` it is one cell of the tracks' spreadsheet: no name, no frame until
// it has the keyboard. `field` is one entry of the editor's view; the value
// goes back through `edited`.
Item {
    id: root

    property var field: ({})
    // A track's "title" is its own; a record's is the record's.
    property bool ofTrack: false
    property bool cell: false

    signal edited(string name, string value)

    readonly property string name: field.name || ""
    readonly property bool toggle: name === "compilation"
    readonly property string invalid: field.invalid || ""
    readonly property bool labelled: !cell && !toggle
    // It has the keyboard.
    readonly property bool editing: input.editing

    implicitWidth: Metrics.selectWidth
    implicitHeight: cell ? Metrics.metadataRow
        : (labelled ? caption.height + Metrics.gap2 : 0) + Metrics.fieldHeight + (invalid !== "" ? note.height + Metrics.gap2 : 0)

    function label(n) {
        switch (n) {
        case "title": return ofTrack ? I18n.tr("Title", I18n.rev) : I18n.tr("Album", I18n.rev)
        case "album_artist": return I18n.tr("Album artist", I18n.rev)
        case "album_artists": return I18n.tr("Album artists", I18n.rev)
        case "compilation": return I18n.tr("Compilation", I18n.rev)
        case "date": return I18n.tr("Date", I18n.rev)
        case "genres": return I18n.tr("Genres", I18n.rev)
        case "catalog_number": return I18n.tr("Catalog number", I18n.rev)
        case "musicbrainz_release_id": return I18n.tr("MusicBrainz release", I18n.rev)
        case "musicbrainz_release_group_id": return I18n.tr("MusicBrainz release group", I18n.rev)
        case "musicbrainz_album_artist_ids": return I18n.tr("MusicBrainz album artists", I18n.rev)
        case "discogs_release_id": return I18n.tr("Discogs release", I18n.rev)
        case "artist_credit": return I18n.tr("Artist", I18n.rev)
        case "artists": return I18n.tr("Artists", I18n.rev)
        case "track_number": return I18n.tr("Track", I18n.rev)
        case "disc_number": return I18n.tr("Disc", I18n.rev)
        case "composers": return I18n.tr("Composers", I18n.rev)
        case "performers": return I18n.tr("Performers", I18n.rev)
        case "musicbrainz_recording_id": return I18n.tr("MusicBrainz recording", I18n.rev)
        case "musicbrainz_track_id": return I18n.tr("MusicBrainz track", I18n.rev)
        case "musicbrainz_artist_ids": return I18n.tr("MusicBrainz artists", I18n.rev)
        }
        return n
    }

    // What the facade holds, unless it is being typed over.
    function refresh() {
        if (!input.editing)
            input.text = field.value || ""
    }
    onFieldChanged: refresh()
    Component.onCompleted: refresh()

    Text {
        id: caption
        visible: root.labelled
        width: parent.width - (layer.visible ? layer.width + Metrics.gap4 : 0)
        text: root.label(root.name)
        elide: Text.ElideRight
        font.pointSize: Type.small
        font.weight: root.field.dirty ? Type.semibold : Type.regular
        color: root.field.dirty ? Theme.color.textPrimary : Theme.color.textSecondary
    }

    Icon {
        id: layer
        visible: root.field.overridden === true && !root.cell
        anchors.right: parent.right
        glyph: Icons.stack2
        size: Metrics.iconXs
        color: Theme.color.textTertiary

        HoverHandler {
            onHoveredChanged: hovered ? TipState.show(I18n.tr("A sidecar sets this, not the file", I18n.rev), layer) : TipState.hide(layer)
        }
    }

    TextField {
        id: input
        visible: !root.toggle
        y: root.labelled ? caption.height + Metrics.gap2 : 0
        width: parent.width
        height: root.cell ? Metrics.metadataRow : Metrics.fieldHeight
        flat: root.cell
        enabled: root.field.editable !== false
        opacity: enabled ? 1 : Metrics.disabledOpacity
        placeholder: root.field.mixed ? I18n.tr("Mixed", I18n.rev) : ""
        border.color: root.invalid !== "" ? Theme.color.statusNegative : Theme.color.accent
        border.width: editing ? (root.cell ? 1 : Metrics.focusRing) : (root.invalid !== "" ? 1 : 0)
        // Every key reaches the form: a Save or a Close pressed while a
        // field still has the keyboard sees what is in it.
        onTextEdited: root.edited(root.name, text)
        onEditingFinished: {
            if (text !== (root.field.value || ""))
                root.edited(root.name, text)
        }
    }

    Row {
        visible: root.toggle
        height: Metrics.fieldHeight
        spacing: Metrics.gap8
        Toggle {
            anchors.verticalCenter: parent.verticalCenter
            checked: root.field.value === "true"
            onToggled: (on) => root.edited(root.name, on ? "true" : "false")
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Compilation (various artists)", I18n.rev)
            font.pointSize: Type.meta
            font.weight: root.field.dirty ? Type.semibold : Type.regular
            color: root.field.dirty ? Theme.color.textPrimary : Theme.color.textSecondary
        }
    }

    Text {
        id: note
        visible: root.invalid !== "" && !root.cell
        y: input.y + input.height + Metrics.gap2
        width: parent.width
        text: root.invalid
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.statusNegative
    }

    // A cell has no room under it: why it cannot be saved shows on hover.
    HoverHandler {
        enabled: root.cell && root.invalid !== ""
        onHoveredChanged: hovered ? TipState.show(root.invalid, input) : TipState.hide(input)
    }
}
