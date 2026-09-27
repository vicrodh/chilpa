import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The metadata editor's tracks as a spreadsheet: disc, number, title and
// artist edited in place, the file for reference, and under the table a bar
// that opens the rest of the chosen track's fields. The table scrolls on its
// own; the page does not. `doc` is the editor's view.
Rectangle {
    id: root

    property var doc: ({})
    // The track whose other fields the bar opens. It starts where the editor
    // was opened from.
    property int chosen: 0
    property bool detailsOpen: false

    readonly property var tracks: doc.tracks || []
    readonly property var track: chosen >= 0 && chosen < tracks.length ? tracks[chosen] : null
    readonly property real table: width - Metrics.gap8 * 2 // derived: the table's inset on each side
    readonly property real artistWidth: Math.max(Metrics.metadataCellMin, table * 0.23) // derived: QBZ's share of the table
    readonly property real fileWidth: Math.max(Metrics.metadataCellMin, table * 0.22) // derived: QBZ's share of the table
    readonly property var widths: [Metrics.metadataDisc, Metrics.metadataTrack,
        Math.max(Metrics.metadataTitleMin, table - Metrics.metadataDisc - Metrics.metadataTrack - artistWidth - fileWidth),
        artistWidth, fileWidth]
    // What the bar opens: every field of the track but the four in the row.
    readonly property var rest: track ? track.fields.filter(f => ["disc_number", "track_number", "title", "artist_credit"].indexOf(f.name) < 0) : []

    radius: Metrics.radiusPanel
    color: Theme.color.surfacePanel
    border.width: 1
    border.color: Theme.color.strokeHairline
    clip: true

    Connections {
        target: MetadataEditor
        function onOpenChanged() {
            if (MetadataEditor.open)
                root.chosen = Math.max(0, root.doc.focus || 0)
        }
    }

    Rectangle {
        id: head
        x: Metrics.gap8
        y: Metrics.gap8
        width: root.table
        height: Metrics.metadataRow
        color: Theme.color.stateStripe
        Row {
            Repeater {
                model: [I18n.tr("Disc", I18n.rev), I18n.tr("Track", I18n.rev), I18n.tr("Title", I18n.rev), I18n.tr("Artist", I18n.rev), I18n.tr("File", I18n.rev)]
                Text {
                    required property string modelData
                    required property int index
                    width: root.widths[index]
                    height: head.height
                    leftPadding: Metrics.metadataCellPad
                    verticalAlignment: Text.AlignVCenter
                    text: modelData
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    font.weight: Type.semibold
                    color: Theme.color.textSecondary
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.color.strokeHairline }
                }
            }
        }
    }

    ListView {
        id: list
        x: Metrics.gap8
        anchors.top: head.bottom
        anchors.bottom: bar.top
        width: root.table
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        // By count, not by list: a new view does not rebuild the rows, so the
        // cell being typed in keeps the keyboard.
        model: root.tracks.length
        delegate: MetadataTrackRow {
            required property int index
            width: ListView.view.width
            rowIndex: index
            widths: root.widths
            track: root.tracks[index]
            chosen: index === root.chosen
            onChoose: root.chosen = index
            onEdited: (trackId, name, value) => MetadataEditor.setTrack(trackId, name, value)
        }
    }

    Rectangle {
        id: bar
        anchors.bottom: details.top
        width: parent.width
        height: Metrics.metadataDetailsBar
        color: barPointer.hovered ? Theme.color.stateHover : Theme.color.stateStripe
        Text {
            x: Metrics.gap12
            width: parent.width - Metrics.gap12 * 3 - Metrics.iconSm // derived: room for the chevron
            anchors.verticalCenter: parent.verticalCenter
            text: root.track ? I18n.trArgs("Track details · {}", [root.track.file], I18n.rev) : ""
            elide: Text.ElideMiddle
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Icon {
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            size: Metrics.iconSm
            glyph: root.detailsOpen ? Icons.chevronDown : Icons.chevronUp
        }
        HoverHandler { id: barPointer; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.detailsOpen = !root.detailsOpen }
    }

    Item {
        id: details
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.detailsOpen ? Metrics.metadataDetails : 0
        clip: true
        Grid {
            x: Metrics.gap12
            y: Metrics.gap8
            columns: 2
            columnSpacing: Metrics.gap12
            rowSpacing: Metrics.gap6
            Repeater {
                model: root.rest.length
                MetadataField {
                    required property int index
                    width: (details.width - Metrics.gap12 * 3) / 2 // derived: two columns between three gaps
                    ofTrack: true
                    field: root.rest[index] || ({})
                    onEdited: (name, value) => MetadataEditor.setTrack(root.track.id, name, value)
                }
            }
        }
    }
}
