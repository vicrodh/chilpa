import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A media server's own settings, as it words them: its quality, when it
// converts music for each device (`dlna::settings`). Each setting shows the
// values the server offers, verbatim, and changes only when Apply is
// pressed (`Servers.begin("choose/...")`); nothing is chosen for the
// person. It is optional and folded until opened, saying what the server
// sends now. A server that offers none shows nothing here.
Column {
    id: quality
    property var setup: ({})
    property bool busy: false
    property bool preview: false
    property bool open: false
    objectName: "serverSettings"
    visible: (setup.settings || []).length > 0
    topPadding: Metrics.gap8
    spacing: Metrics.gap8
    PillButton {
        objectName: "serverSettingsToggle"
        kind: "ghost"
        glyph: quality.open ? Icons.chevronDown : Icons.chevronRight
        text: I18n.tr("Server quality (optional)", I18n.rev)
        onClicked: quality.open = !quality.open
    }
    Text {
        objectName: "serverSettingsNow"
        visible: !quality.open
        width: parent.width
        text: (quality.setup.settings || []).map(setting => setting.title).join(" · ")
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }
    Text {
        visible: quality.open
        width: parent.width
        text: I18n.tr("This server converts music for each device. Choose the quality you want from what it offers. A higher bitrate sounds better; these values and their limits are the server's own.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Repeater {
        model: quality.setup.settings || []
        Column {
            id: setting
            visible: quality.open
            required property var modelData
            required property int index
            width: quality.width
            spacing: Metrics.gap4
            Text {
                width: parent.width
                text: setting.modelData.title
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Row {
                spacing: Metrics.gap8
                SelectField {
                    id: pick
                    objectName: "serverSettingValue_" + setting.index
                    width: quality.width / 2 // derived: half the row, the button beside it
                    placeholder: I18n.tr("Choose a value", I18n.rev)
                    options: setting.modelData.values.map(v => v.title)
                    values: setting.modelData.values.map(v => v.id)
                    onPicked: chosen => pick.value = chosen
                }
                PillButton {
                    objectName: "serverSettingApply_" + setting.index
                    text: I18n.tr("Apply", I18n.rev)
                    enabled: pick.value !== "" && !quality.busy && !quality.preview
                    onClicked: Servers.begin("choose/" + pick.value)
                }
            }
        }
    }
    Text {
        objectName: "serverSettingSaid"
        visible: quality.open && (quality.setup.settingSaid || "") !== ""
        width: parent.width
        text: quality.setup.settingSaid || ""
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
