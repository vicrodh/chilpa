// hot-path: one per record and per track of the Offline music page.
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A record of the Offline music page, or one of its tracks, as QBZ's
// offline cache draws them: a record has its cover, its artist and what it
// keeps; a track its box, its number, its artist and its length. Both
// offer checking their copies again and removing them, and say how they
// stand. A track that failed says why under its title.
Item {
    id: line
    property var row: ({})
    property bool checked: false
    property bool busy: false
    readonly property bool record: row.kind === "album"
    readonly property bool gone: record ? row.gone === true : (row.id || 0) <= 0
    readonly property var ids: record ? (row.ids || []) : (gone ? [] : [row.id])
    signal toggled()
    signal retried(var ids)
    signal removed(var ids)
    implicitHeight: record ? Metrics.offlineRecordRow : Metrics.offlineTrackRow

    HoverHandler { id: hover }
    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: line.record ? Theme.color.surfaceRaised : hover.hovered ? Theme.color.stateHover : "transparent"
    }
    Item {
        id: lead
        x: Metrics.gap10
        width: line.record ? Metrics.artRow : Metrics.button28 * 2 // derived: the box and the number
        height: parent.height
        ArtTile {
            visible: line.record
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.artRow; height: Metrics.artRow
            path: line.row.art || ""
            radius: Metrics.radiusArtSmall
        }
        Item {
            objectName: "offlineCheck"
            visible: !line.record && !line.gone
            width: Metrics.button28; height: parent.height
            Icon {
                anchors.centerIn: parent
                glyph: line.checked ? Icons.circleCheckFilled : Icons.circle
                filled: line.checked
                size: Metrics.iconMd
                color: line.checked ? Theme.color.accent : Theme.color.textSecondary
            }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: line.toggled() }
        }
        Text {
            visible: !line.record
            x: Metrics.button28
            width: Metrics.button28; height: parent.height
            text: line.row.number > 0 ? String(line.row.number) : ""
            horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
            font.pointSize: Type.body
            color: Theme.color.textTertiary
        }
    }
    Column {
        anchors.left: lead.right; anchors.leftMargin: Metrics.gap12
        anchors.right: tail.left; anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2
        Text {
            objectName: "offlineRowTitle"
            width: parent.width
            text: line.row.title || (line.record ? (line.gone ? I18n.tr("Not in the library", I18n.rev) : I18n.tr("Unknown album", I18n.rev)) : I18n.tr("Unknown title", I18n.rev))
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pointSize: line.record ? Type.bodyLarge : Type.body
            font.weight: line.record ? Type.semibold : Type.regular
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            readonly property bool failed: !line.record && line.row.state === "failed" && (line.row.error || "") !== ""
            text: failed ? line.row.error : (line.row.artist || (line.gone ? "" : I18n.tr("Unknown artist", I18n.rev)))
            visible: text !== ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: failed ? Theme.color.statusNegative : Theme.color.textSecondary
        }
    }
    Row {
        id: tail
        anchors.right: parent.right; anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: line.record ? I18n.trCount("{} track", "{} tracks", line.row.tracks || 0, I18n.rev) + " · " + (line.row.size || "") : (line.row.duration || "")
            textFormat: Text.PlainText
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        IconButton {
            objectName: "offlineRetry"
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.refresh
            tip: I18n.tr("Check again, and download again if needed", I18n.rev)
            enabled: !line.busy && line.ids.length > 0
            onClicked: line.retried(line.ids)
        }
        IconButton {
            objectName: "offlineRemove"
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.trash
            tip: I18n.tr("Remove downloaded files…", I18n.rev)
            enabled: !line.busy && line.ids.length > 0
            onClicked: line.removed(line.ids)
        }
        Icon {
            objectName: "offlineState"
            anchors.verticalCenter: parent.verticalCenter
            readonly property string stage: line.row.state || ""
            glyph: stage === "complete" ? Icons.circleCheck : stage === "failed" ? Icons.alertTriangle : Icons.cloudDownload
            size: Metrics.iconMd
            color: stage === "complete" ? Theme.color.accent : stage === "failed" ? Theme.color.statusWarning : Theme.color.textTertiary
        }
    }
}
