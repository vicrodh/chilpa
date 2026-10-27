import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: row
    property var entry: ({})
    property bool saved: false
    implicitHeight: Math.max(label.implicitHeight, runtime.implicitHeight, preference.implicitHeight) + Metrics.gap12
    Text {
        id: label
        width: parent.width / 3 // derived: a third for each diagnostic column
        text: row.entry.label || ""
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
        wrapMode: Text.WordWrap
    }
    Text {
        id: preference
        x: label.width + Metrics.gap8
        width: row.saved ? label.width - Metrics.gap16 : 0
        visible: row.saved
        text: row.entry.saved || "—"
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
        wrapMode: Text.WrapAnywhere
    }
    Text {
        id: runtime
        x: row.saved ? preference.x + preference.width + Metrics.gap8 : preference.x
        width: parent.width - x
        text: row.entry.runtime || "—"
        font.pointSize: Type.meta
        color: row.entry.status === "different" ? Theme.color.statusWarning : Theme.color.textPrimary
        wrapMode: Text.WrapAnywhere
    }
}
