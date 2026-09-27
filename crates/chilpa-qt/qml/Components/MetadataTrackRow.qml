import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// One track in the metadata editor: disc, number, title and artist, each
// editable in place, and the file it lives in. A click chooses it, and the
// editor shows the rest of its fields under the list. `track` is one entry
// of the editor's view.
ListRowSurface {
    id: root

    property var track: ({ fields: [] })
    property bool chosen: false

    signal choose()
    signal edited(int trackId, string name, string value)

    function field(name) {
        const all = track.fields || []
        for (let i = 0; i < all.length; ++i) {
            if (all[i].name === name)
                return all[i]
        }
        return { name: name, value: "" }
    }
    readonly property bool faulty: (track.fields || []).some(f => f.invalid !== "")
    readonly property bool changed: (track.fields || []).some(f => f.dirty)

    implicitHeight: Metrics.metadataRow
    selected: chosen
    hovered: pointer.hovered

    HoverHandler { id: pointer }
    TapHandler { onTapped: root.choose() }

    Row {
        anchors.fill: parent
        anchors.leftMargin: Metrics.gap8
        anchors.rightMargin: Metrics.gap8
        spacing: Metrics.gap6

        Repeater {
            model: ["disc_number", "track_number"]
            MetadataField {
                required property string modelData
                bare: true
                ofTrack: true
                width: Metrics.metadataNumber
                anchors.verticalCenter: parent.verticalCenter
                field: root.field(modelData)
                onEdited: (name, value) => root.edited(root.track.id, name, value)
            }
        }
        Repeater {
            model: ["title", "artist_credit"]
            MetadataField {
                required property string modelData
                bare: true
                ofTrack: true
                width: (root.width - Metrics.metadataNumber * 2 - file.width - marks.width - Metrics.gap6 * 5 - Metrics.gap8 * 2) / 2 // derived: what the fixed cells leave, in halves
                anchors.verticalCenter: parent.verticalCenter
                field: root.field(modelData)
                onEdited: (name, value) => root.edited(root.track.id, name, value)
            }
        }
        Text {
            id: file
            width: Metrics.selectWidth / 2 // derived: half a field
            anchors.verticalCenter: parent.verticalCenter
            text: root.track.file || ""
            elide: Text.ElideMiddle
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        Icon {
            id: marks
            anchors.verticalCenter: parent.verticalCenter
            size: Metrics.iconSm
            glyph: root.faulty ? Icons.alertTriangle : root.changed ? Icons.circleCheck : ""
            color: root.faulty ? Theme.color.statusNegative : Theme.color.textSecondary
        }
    }
}
