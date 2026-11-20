import QtQuick
import "../Theme"

// A screen's head (design/02): its title, a count in the room left beside
// it, and its tools at the right. When the three do not fit on one line,
// the tools take a line of their own under the title and the head grows.
// A flexible tool (a search field) takes what the others leave, no less
// than `flexibleLeast` beside the title.
Item {
    id: head

    property string title
    property string count
    // The count at the right end of its room, as a total rather than a label.
    property bool countRight: false
    property Item flexible: null
    property real flexibleLeast: Metrics.folderTreeMin
    property Item countTool: null
    default property alias tools: tools.data

    // What the tools other than the flexible one take, gaps included.
    readonly property real others: {
        let wide = 0
        for (const item of tools.children) if (item !== flexible && item.visible) wide += item.width + tools.spacing
        return wide
    }
    readonly property real countWidth: tally.implicitWidth + (countTool ? countTool.width + Metrics.gap6 : 0)
    readonly property bool stacked: name.implicitWidth + countWidth + Metrics.gap12 * 2 + (flexible ? others + flexibleLeast : tools.width) > width // derived: a gap on each side of the count
    // The flexible tool's room: beside the title, what the title, the count
    // and the other tools leave; on a line of its own, what the others leave.
    readonly property real flexibleRoom: stacked ? width - others : width - name.width - countWidth - Metrics.gap12 * 2 - others // derived: a gap on each side of the count

    implicitHeight: stacked ? Metrics.shelfHeader * 2 : Metrics.shelfHeader // derived: a second line for the tools

    Text {
        id: name
        y: (Metrics.shelfHeader - height) / 2 // derived: centred on the first line
        width: Math.min(implicitWidth, head.width)
        text: head.title
        elide: Text.ElideRight
        font.pointSize: Type.title
        font.weight: Type.bold
        font.letterSpacing: Type.titleTracking
        color: Theme.color.textPrimary
    }
    Item {
        id: countLine
        anchors.left: name.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: head.stacked ? parent.right : tools.left
        anchors.rightMargin: head.stacked ? 0 : Metrics.gap12
        height: Metrics.shelfHeader

        Text {
            id: tally
            anchors.left: parent.left
            anchors.right: countAccessory.left
            anchors.rightMargin: head.countTool ? Metrics.gap6 : 0
            y: head.countRight ? (parent.height - height) / 2 : name.y + name.baselineOffset - baselineOffset
            horizontalAlignment: head.countRight ? Text.AlignRight : Text.AlignLeft
            text: head.count
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Item {
            id: countAccessory
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: head.countTool ? head.countTool.width : 0
            height: head.countTool ? head.countTool.height : 0
        }
    }
    onCountToolChanged: if (countTool) countTool.parent = countAccessory

    Row {
        id: tools
        anchors.right: parent.right
        y: (head.stacked ? Metrics.shelfHeader : 0) + (Metrics.shelfHeader - height) / 2 // derived: centred on its line
        spacing: Metrics.gap12
    }
}
