import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: footer
    property var state: ({})
    property var selection: ({})
    property bool active: false
    property string presetName: ""
    spacing: Metrics.gap8
    Text {
        width: parent.width
        visible: !!footer.state.error
        text: footer.state.error || ""
        wrapMode: Text.WordWrap
        color: Theme.color.statusNegative
        font.pointSize: Type.small
    }
    Text {
        objectName: "dspStatus"
        width: parent.width
        text: footer.state.blocked || ((footer.active ? I18n.tr("DSP active", I18n.rev) : footer.selection.enabled ? I18n.tr("Ready for local PCM playback", I18n.rev) : I18n.tr("Bypass · original samples", I18n.rev)) + (footer.presetName ? " · " + footer.presetName : ""))
        elide: Text.ElideRight
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Text {
        objectName: "dspDisclaimer"
        width: parent.width
        text: I18n.tr("DSP changes the signal and breaks strict bit-perfect playback.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
