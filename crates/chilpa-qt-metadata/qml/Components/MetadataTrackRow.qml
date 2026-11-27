import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One track in the metadata editor's spreadsheet: the mark of the work it
// belongs to, disc, number, title and artist, each a cell edited in place,
// and the file it lives in, read only. A click, or a cell taking the
// keyboard, chooses it; the editor shows the rest of its fields under the
// table. `track` is one entry of the editor's view; the column widths come
// from the table so every row lines up.
ListRowSurface {
    id: root

    property var track: ({ fields: [] })
    property bool chosen: false
    // Which of the editor's works this track belongs to, in the order they
    // first appear: it picks the mark's colours. -1 when it belongs to none.
    property int workIndex: -1
    property var widths: [Metrics.metadataWork, Metrics.metadataDisc, Metrics.metadataTrack, Metrics.metadataTitleMin, Metrics.metadataCellMin, Metrics.metadataCellMin]
    readonly property string work: field("work").value || ""

    signal choose()
    signal edited(int trackId, string name, string value)
    // A double click on the work's mark: its field, ready to type.
    signal openWork()

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
        // A W in the work's colours: the same work, the same mark. Its name
        // shows on hover; a double click opens it to be changed or emptied.
        Item {
            id: workCell
            objectName: "metadataWorkCell"
            width: root.widths[0]
            height: parent.height
            readonly property var mark: root.workIndex < 0 ? null : Theme.workMarks[root.workIndex % Theme.workMarks.length]
            Rectangle {
                objectName: "metadataWorkMark"
                visible: root.work !== "" && workCell.mark !== null
                anchors.centerIn: parent
                width: Metrics.metadataWorkMark
                height: Metrics.metadataWorkMark
                radius: Metrics.radiusRow
                color: workCell.mark ? workCell.mark.bg : "transparent"
                Text {
                    anchors.centerIn: parent
                    // A mark, not a word: the same W in every language.
                    text: "W"
                    font.pointSize: Type.small
                    font.weight: Type.bold
                    color: workCell.mark ? workCell.mark.fg : Theme.color.textPrimary
                }
            }
            // A click chooses the row, as anywhere in it; a double click is
            // the work's.
            MouseArea {
                anchors.fill: parent
                enabled: root.work !== ""
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onContainsMouseChanged: containsMouse ? TipState.show(root.work, workCell) : TipState.hide(workCell)
                onClicked: root.choose()
                onDoubleClicked: root.openWork()
            }
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.color.strokeHairline }
        }
        Repeater {
            model: ["disc_number", "track_number", "title", "artist_credit"]
            MetadataField {
                required property string modelData
                required property int index
                width: root.widths[index + 1]
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
            width: root.widths[5] // derived: the file column, the last of the table
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
