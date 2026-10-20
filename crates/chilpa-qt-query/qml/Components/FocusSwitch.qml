import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

SegmentedControl {
    id: control
    objectName: "focusSwitch"
    property bool focusMode: SettingsModel.kept("ui.library.focus") === "true"
    current: SettingsModel.panesOpen ? (focusMode ? 1 : 0) : -1
    options: [{ text: I18n.tr("Columns", I18n.rev) }, { text: I18n.tr("Focus", I18n.rev) }]
    onActivated: (index) => {
        const same = focusMode === (index === 1)
        focusMode = index === 1
        SettingsModel.keep("ui.library.focus", focusMode ? "true" : "false")
        SettingsModel.panesOpen = same ? !SettingsModel.panesOpen : true
    }
}
