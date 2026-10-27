import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: row
    property bool heading: false
    property string name: ""
    property string detail: ""
    property string status: ""
    property var server: ({})
    property var options: []
    property var values: []
    signal actionRequested(string action)
    implicitHeight: heading ? Metrics.settingsGroup : Metrics.rowRecord

    SourceIcon {
        id: logo
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.iconPlay
        height: width
        visible: !row.heading
        monochrome: true
        sourceInfo: row.server
    }
    Column {
        anchors.left: logo.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: product.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4
        Text {
            width: parent.width
            text: row.heading ? I18n.tr("Name", I18n.rev) : row.name
            elide: Text.ElideRight
            font.pointSize: row.heading ? Type.meta : Type.body
            font.weight: Type.medium
            color: row.heading ? Theme.color.textSecondary : Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: !row.heading
            text: row.status
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        HoverHandler { onHoveredChanged: hovered && !row.heading ? TipState.show(row.detail, parent) : TipState.hide(parent) }
    }
    Text {
        id: product
        anchors.right: tracks.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        // I reserve one quarter of the table for the product and version.
        width: row.width / 4 // derived: a quarter for product and version
        text: row.heading ? I18n.tr("Type / version", I18n.rev) : row.server.product || "—"
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: row.heading ? Theme.color.textSecondary : Theme.color.textPrimary
        HoverHandler { onHoveredChanged: hovered && !row.heading ? TipState.show(product.text, parent) : TipState.hide(parent) }
    }
    Text {
        id: tracks
        anchors.right: actions.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.settingsPercent + Metrics.gap24
        horizontalAlignment: Text.AlignRight
        text: row.heading ? I18n.tr("# Tracks", I18n.rev) : Number(row.server.tracks || 0).toLocaleString(Qt.locale(), 'f', 0)
        font.pointSize: Type.meta
        color: row.heading ? Theme.color.textSecondary : Theme.color.textPrimary
    }
    Item {
        id: actions
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.selectWidth / 2 // derived: compact action selector
        height: Metrics.fieldHeightLarge
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: row.heading
            text: I18n.tr("Actions", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        SelectField {
            objectName: "serverActions"
            anchors.fill: parent
            visible: !row.heading
            placeholder: I18n.tr("Actions", I18n.rev)
            options: row.options
            values: row.values
            onPicked: value => row.actionRequested(value)
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: Metrics.hairline
        color: Theme.color.strokeHairline
    }
}
