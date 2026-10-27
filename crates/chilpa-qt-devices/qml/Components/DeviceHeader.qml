import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Column {
    id: head
    property var document: ({})
    property var status: ({})
    property bool showStorageNotice: true
    signal profileAsked(string id)
    signal destinationAsked(string id)
    signal refreshAsked()
    signal createAsked()
    spacing: Metrics.gap12

    Row {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: Math.max(0, parent.width - actions.width - parent.spacing)
            text: I18n.tr("Device sync", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
            elide: Text.ElideRight
        }
        Row {
            id: actions
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap6
            PillButton { objectName: "deviceAddStorage"; text: I18n.tr("Add storage", I18n.rev); enabled: !head.status.busy; onClicked: head.createAsked() }
            IconButton { objectName: "deviceRefresh"; glyph: Icons.refresh; tip: I18n.tr("Refresh connected players", I18n.rev); enabled: !head.status.busy; onClicked: head.refreshAsked() }
        }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap12
        Column {
            visible: head.document.has_profiles === true
            width: Math.min(DeviceLayout.selectWidth, head.document.has_profiles ? (parent.width - Metrics.gap12) / 2 : parent.width) // derived: two selectors share the row
            spacing: Metrics.gap4
            Text { text: I18n.tr("Saved profile", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.small }
            SelectField {
                objectName: "deviceProfile"
                width: parent.width
                options: head.document.profile_names || []
                values: head.document.profile_values || []
                value: head.status.profile || ""
                enabled: !head.status.busy
                onPicked: value => head.profileAsked(value)
            }
        }
        Column {
            width: Math.min(DeviceLayout.selectWidth, head.document.has_profiles ? (parent.width - Metrics.gap12) / 2 : parent.width) // derived: two selectors share the row
            spacing: Metrics.gap4
            Text { text: I18n.tr("Player and storage", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.small }
            SelectField {
                objectName: "deviceDestination"
                width: parent.width
                options: head.document.destination_names || []
                values: head.document.destination_values || []
                details: head.document.destination_details || []
                enabledOptions: head.document.destination_enabled || []
                value: head.status.destination || ""
                placeholder: I18n.tr("Choose storage", I18n.rev)
                enabled: !head.status.busy
                onPicked: value => head.destinationAsked(value)
            }
        }
    }
    Text {
        width: parent.width
        visible: text !== ""
        text: [head.document.storage_detail, head.document.profile_folder].filter(Boolean).join(" · ")
        textFormat: Text.PlainText
        font.pointSize: Type.small
        color: Theme.color.textSecondary
        elide: Text.ElideMiddle
    }
    Text {
        objectName: "deviceStorageNotice"
        width: parent.width
        visible: head.showStorageNotice && !head.status.error && text !== ""
        text: head.document.storage_notice || ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
        font.pointSize: Type.small
    }
    CutTextTips { watching: head }
}
