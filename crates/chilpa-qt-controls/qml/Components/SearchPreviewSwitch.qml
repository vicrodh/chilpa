import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Whether results show under the search box as you type, the same switch as
// Settings > Navigation > Search, in the flyout and on the results page.
Row {
    id: preview
    required property SearchModel search
    readonly property bool on: Prefs.rev >= 0 && SettingsModel.kept("navigation.searchPreview") !== "false"
    spacing: Metrics.gap8
    Toggle {
        id: toggle
        objectName: "searchPreviewSwitch"
        compact: true
        anchors.verticalCenter: parent.verticalCenter
        checked: preview.on
        Accessible.name: label.text
        onToggled: on => preview.search.keepPreview(on)
    }
    Text {
        id: label
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Search preview", I18n.rev)
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
}
