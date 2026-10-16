import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The history's column titles over the widths its rows use: time, track,
// skipped, quality and bit-perfect.
Item {
    id: head

    height: Metrics.rowHeader

    Text {
        id: clock
        x: Charts.historyInset
        width: Charts.timeColumn
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Time", I18n.rev)
        font.pointSize: Type.label
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
    Text {
        anchors.left: clock.right
        anchors.right: skipped.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Track", I18n.rev)
        elide: Text.ElideRight
        font.pointSize: Type.label
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
    Text {
        id: skipped
        anchors.right: quality.left
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.skippedColumn
        text: I18n.tr("Skipped", I18n.rev)
        elide: Text.ElideRight
        font.pointSize: Type.label
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
    Text {
        id: quality
        anchors.right: perfect.left
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.qualityColumn
        text: I18n.tr("Quality", I18n.rev)
        elide: Text.ElideRight
        font.pointSize: Type.label
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
    Text {
        id: perfect
        anchors.right: parent.right
        anchors.rightMargin: Charts.historyInset
        anchors.verticalCenter: parent.verticalCenter
        width: Charts.perfectColumn
        horizontalAlignment: Text.AlignHCenter
        text: I18n.tr("BP", I18n.rev)
        font.pointSize: Type.label
        font.weight: Type.medium
        color: Theme.color.textSecondary

        HoverHandler {
            onHoveredChanged: hovered ? TipState.show(I18n.tr("Bit-perfect", I18n.rev), perfect) : TipState.hide(perfect)
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }
}
