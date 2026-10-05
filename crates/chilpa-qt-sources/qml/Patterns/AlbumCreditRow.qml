import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Column {
    id: row
    property var credit: ({})
    property bool expanded: false
    readonly property bool rich: SettingsModel.density === "rich"
    readonly property bool hasDetails: !!credit.performers || !!credit.copyright || (!rich && !!credit.artist)
    spacing: Metrics.gap8
    Rectangle {
        width: parent.width; height: Math.max(row.rich ? Metrics.rowRich : Metrics.rowCompact, title.height + (row.rich ? Metrics.gap16 : Metrics.gap4))
        radius: Metrics.radiusRow; color: hover.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
        Column {
            id: title; x: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter
            width: parent.width - toggle.width - Metrics.gap24; spacing: Metrics.gap4
            Text { width: parent.width; text: row.credit.title || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.medium }
            Text { width: parent.width; visible: row.rich && !!row.credit.artist; text: row.credit.artist || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.small }
        }
        HoverHandler { id: hover; cursorShape: row.hasDetails ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { enabled: row.hasDetails; onTapped: row.expanded = !row.expanded }
        IconButton {
            id: toggle; size: row.rich ? Metrics.button28 : Metrics.rowCompact; objectName: "expandCredit"; anchors.right: parent.right; anchors.rightMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter
            visible: row.hasDetails; glyph: row.expanded ? Icons.chevronUp : Icons.chevronDown
            tip: row.expanded ? I18n.tr("Collapse", I18n.rev) : I18n.tr("Expand", I18n.rev)
            onClicked: row.expanded = !row.expanded
        }
    }
    TextEdit {
        objectName: "creditText"; width: parent.width; visible: row.expanded && row.hasDetails
        text: [row.rich ? "" : row.credit.artist || "", row.credit.performers || "", row.credit.copyright || ""].filter(Boolean).join("\n\n")
        textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
        color: Theme.color.textSecondary; selectionColor: Theme.color.accent; font.pointSize: Type.body
    }
}
