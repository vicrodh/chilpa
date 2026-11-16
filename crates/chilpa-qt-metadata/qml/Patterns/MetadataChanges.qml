import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Column {
    id: preview
    property var changes: []
    property bool busy: false
    readonly property int selectedCount: changes.filter(change => change.selected === true && !(change.invalid || "")).length
    signal choose(string key, bool selected)
    spacing: Metrics.gap8
    Text { width:parent.width; text:I18n.tr("Review changes",I18n.rev); font.pointSize:Type.section; font.weight:Type.semibold; color:Theme.color.textPrimary }
    Text { width:parent.width; text:I18n.tr("Existing genres are unselected by default.",I18n.rev); wrapMode:Text.WordWrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
    ListView {
        objectName: "metadataChanges"
        width: parent.width
        height: Math.min(preview.changes.length, Metrics.metadataPreviewRows) * Metrics.metadataChangeHeight
        // Keep delegates and keyboard focus when only a selection changes.
        model: preview.changes.length; clip:true; reuseItems:true
        cacheBuffer: Metrics.metadataChangeHeight
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        delegate: MetadataChangeRow {
            required property int index
            width: ListView.view.width; height:implicitHeight
            rowIndex:index; change:preview.changes[index] || ({}); busy:preview.busy
            onChoose: (key, selected) => preview.choose(key, selected)
        }
    }
    Text { width:parent.width; visible:preview.changes.length === 0; text:I18n.tr("No changes to apply.",I18n.rev); wrapMode:Text.WordWrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
}
