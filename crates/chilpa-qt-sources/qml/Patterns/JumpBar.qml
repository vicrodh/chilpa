import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The strip under an artist's header: "JUMP TO" and one entry per block of
// the page, with a search over the page on the right. It sticks to the top
// while the page scrolls under it.
Rectangle {
    id: bar
    property var entries: []
    property string active: ""
    property alias search: field.text
    signal jump(string id)
    height: Metrics.jumpBar
    // Under the dynamic background it lets the field through, as QBZ's does.
    color: Theme.ambient ? Theme.color.ambientBar : Theme.color.surfaceWindow
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Row {
        anchors.left: parent.left; anchors.right: field.left; anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap16; clip: true
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("JUMP TO", I18n.rev); font.pointSize: Type.label; font.weight: Type.semibold; font.letterSpacing: Type.labelTracking; color: Theme.color.textTertiary }
        Repeater {
            model: bar.entries
            Item {
                id: tab
                required property var modelData
                readonly property bool current: bar.active === modelData.id
                width: label.implicitWidth; height: bar.height
                Text {
                    id: label; anchors.verticalCenter: parent.verticalCenter; text: tab.modelData.title || ""; textFormat: Text.PlainText
                    font.pointSize: Type.meta; font.weight: tab.current ? Type.semibold : Type.regular
                    color: tab.current || hover.hovered ? Theme.color.textPrimary : Theme.color.textSecondary
                }
                Rectangle { visible: tab.current; anchors.bottom: parent.bottom; width: parent.width; height: Metrics.gap2; color: Theme.color.accent }
                HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: bar.jump(tab.modelData.id) }
            }
        }
    }
    SearchField { id: field; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; width: Metrics.screenSearch; placeholder: I18n.tr("Search this page…", I18n.rev) }
}
