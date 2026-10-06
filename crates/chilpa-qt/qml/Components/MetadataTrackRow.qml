import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// One track in the metadata editor's spreadsheet: disc, number, title and
// artist, each a cell edited in place, and the file it lives in, read only.
// A click, or a cell taking the keyboard, chooses it; the editor shows the
// rest of its fields under the table. `track` is one entry of the editor's
// view; the column widths come from the table so every row lines up.
ListRowSurface {
    id: root

    property var track: ({ fields: [] })
    property bool chosen: false
    property var widths: [Metrics.metadataDisc, Metrics.metadataTrack, Metrics.metadataTitleMin, Metrics.metadataCellMin, Metrics.metadataCellMin]

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

    implicitHeight: Metrics.metadataRow
    selected: chosen
    hovered: pointer.hovered

    HoverHandler { id: pointer }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: root.choose() }

    // The chosen row's mark, as a spreadsheet's.
    Rectangle {
        width: root.chosen ? Metrics.focusRing : 0
        height: parent.height
        color: Theme.color.accent
    }

    Row {
        anchors.fill: parent
        Repeater {
            model: ["disc_number", "track_number", "title", "artist_credit"]
            MetadataField {
                required property string modelData
                required property int index
                width: root.widths[index]
                cell: true
                ofTrack: true
                field: root.field(modelData)
                onEdited: (name, value) => root.edited(root.track.id, name, value)
                onEditingChanged: if (editing) root.choose()

                Rectangle {
                    anchors.right: parent.right
                    width: 1
                    height: parent.height
                    color: Theme.color.strokeHairline
                }
            }
        }
        Text {
            width: root.widths[4] // derived: the file column, the last of the table
            height: parent.height
            leftPadding: Metrics.metadataCellPad
            rightPadding: Metrics.metadataCellPad
            verticalAlignment: Text.AlignVCenter
            text: root.track.file || ""
            elide: Text.ElideMiddle
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }
}
