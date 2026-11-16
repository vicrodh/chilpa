import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Rectangle {
    id: navigation
    property var sections: []
    property string section: "graphic"
    signal selected(string section)
    function step(offset) {
        const current = sections.findIndex(entry => entry.key === section)
        const next = Math.max(0, Math.min(sections.length - 1, current + offset))
        selected(sections[next].key)
        entries.itemAt(next)?.forceActiveFocus(Qt.TabFocusReason)
    }
    color: Theme.color.surfacePanel
    radius: Metrics.radiusControl
    Controls.ScrollView {
        anchors.fill: parent
        anchors.margins: Metrics.gap8
        contentWidth: availableWidth
        Column {
            width: parent.width
            spacing: Metrics.gap4
            Repeater {
                id: entries
                model: navigation.sections
                NavItem {
                    id: entry
                    required property int index
                    required property var modelData
                    objectName: "dspSection_" + modelData.key
                    width: parent.width
                    height: Metrics.dspNavigationHeight
                    text: modelData.label
                    glyph: modelData.glyph
                    active: navigation.section === modelData.key
                    activeFocusOnTab: true
                    Accessible.role: Accessible.PageTab
                    Accessible.name: text
                    Accessible.selected: active
                    onClicked: navigation.selected(modelData.key)
                    Keys.onReturnPressed: clicked()
                    Keys.onSpacePressed: clicked()
                    Keys.onUpPressed: navigation.step(-1)
                    Keys.onDownPressed: navigation.step(1)
                    FocusRing {}
                }
            }
        }
    }
}
