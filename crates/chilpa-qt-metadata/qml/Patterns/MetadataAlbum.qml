import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.metadata
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The record's column in the metadata editor: looking it up first, then,
// under a faint line, its cover and every field of the record, each with its
// name above it -- the date and the catalog number share a row, and the
// providers' identifiers fold away. It scrolls on its own. `doc` is the
// editor's view.
Rectangle {
    id: root

    property var doc: ({})
    property bool identifiersOpen: false

    // A record's field by name. The fields stay put as the view changes, so
    // the one being typed in keeps the keyboard.
    function field(name) {
        const all = doc.album || []
        for (let i = 0; i < all.length; ++i) {
            if (all[i].name === name)
                return all[i]
        }
        return { name: name, value: "" }
    }
    function edit(name, value) {
        MetadataEditor.setAlbum(name, value)
    }

    radius: Metrics.radiusPanel
    color: Theme.color.surfacePanel
    border.width: 1
    border.color: Theme.color.strokeHairline
    clip: true

    Flickable {
        id: view
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

        Column {
            id: column
            width: view.width
            spacing: Metrics.gap10

            MetadataLookup { width: parent.width; doc: root.doc }
            Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }
            MetadataArtwork { width: parent.width; doc: root.doc }

            MetadataField { width: parent.width; field: root.field("title"); onEdited: (n, v) => root.edit(n, v) }
            MetadataField { width: parent.width; field: root.field("album_artist"); onEdited: (n, v) => root.edit(n, v) }
            MetadataField { width: parent.width; field: root.field("album_artists"); onEdited: (n, v) => root.edit(n, v) }
            MetadataField { width: parent.width; field: root.field("compilation"); onEdited: (n, v) => root.edit(n, v) }
            MetadataField { width: parent.width; field: root.field("genres"); onEdited: (n, v) => root.edit(n, v) }
            Row {
                width: parent.width
                spacing: Metrics.gap8
                MetadataField { width: (parent.width - parent.spacing) * 0.34; field: root.field("date"); onEdited: (n, v) => root.edit(n, v) } // derived: QBZ's split of the row
                MetadataField { width: (parent.width - parent.spacing) * 0.66; field: root.field("catalog_number"); onEdited: (n, v) => root.edit(n, v) } // derived: QBZ's split of the row
            }

            Rectangle {
                width: parent.width
                height: Metrics.fieldHeight
                radius: Metrics.radiusControl
                color: fold.hovered ? Theme.color.stateHover : Theme.color.stateStripe
                Text {
                    x: Metrics.fieldPadding
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Provider identifiers", I18n.rev)
                    font.pointSize: Type.meta
                    color: Theme.color.textSecondary
                }
                Icon {
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.fieldPadding
                    anchors.verticalCenter: parent.verticalCenter
                    size: Metrics.iconSm
                    glyph: root.identifiersOpen ? Icons.chevronUp : Icons.chevronDown
                }
                HoverHandler { id: fold; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.identifiersOpen = !root.identifiersOpen }
            }
            Column {
                visible: root.identifiersOpen
                width: parent.width
                spacing: Metrics.gap10
                Repeater {
                    model: ["musicbrainz_release_id", "musicbrainz_release_group_id", "musicbrainz_album_artist_ids", "discogs_release_id"]
                    MetadataField {
                        required property string modelData
                        width: parent.width
                        field: root.field(modelData)
                        onEdited: (n, v) => root.edit(n, v)
                    }
                }
            }
        }
    }
}
