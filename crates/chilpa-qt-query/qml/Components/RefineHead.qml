import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// The panel's head (F2, F4): "Match all | any" once two conditions stand,
// the empty line while none does; at the right the live count, the console
// toggle, Undo, "Clear all" and, wide, the button that opens Refine in its
// own window.
Item {
    id: head
    property int conditions: 0
    property string mode: "all"
    // The tracks the query leaves, as the list says it: "23,216 songs · 1625 h".
    property string summary: ""
    // The source still loading, by name, or nothing.
    property string loading: ""
    property bool consoleShown: false
    property bool canUndo: false
    property bool canDetach: true
    signal modeChosen(string mode)
    signal consoleToggled()
    signal undoAsked()
    signal clearAsked()
    signal detachAsked()
    signal saveAsked()
    signal savedAsked()
    property bool canSave: false
    function restoreSavedFocus(saving) { (saving ? saveButton : savedButton).forceActiveFocus() }

    readonly property string countWord: summary.split(" ")[0]
    readonly property real mainHeight: Math.max(Refine.headMinTall, Math.max(left.implicitHeight, right.implicitHeight) + Refine.headTop + Refine.headBottom)
    implicitHeight: mainHeight

    Row {
        id: left
        x: Refine.headLeft
        y: (head.mainHeight - height) / 2 // derived: center within the first header row
        spacing: Refine.headApart
        Text {
            visible: head.conditions > 1
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Match", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Segments {
            objectName: "refineMatch"
            visible: head.conditions > 1
            anchors.verticalCenter: parent.verticalCenter
            tall: Refine.matchSegTall
            pad: Refine.matchSegPad
            options: [{ text: I18n.tr("all", I18n.rev) }, { text: I18n.tr("any", I18n.rev) }]
            current: head.mode === "any" ? 1 : 0
            onActivated: (index) => head.modeChosen(index === 1 ? "any" : "all")
        }
        Text {
            objectName: "refineEmpty"
            visible: head.conditions === 0
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, head.width - right.width - Refine.headLeft - Refine.headApart * 2) // derived: room left of the count
            text: I18n.tr("No conditions yet. Add one below, or click values in Columns.", I18n.rev)
            font.pointSize: Type.row
            color: Theme.color.textSecondary
            elide: Text.ElideRight
        }
    }
    Row {
        id: right
        anchors.right: parent.right
        anchors.rightMargin: Refine.headRight
        y: (head.mainHeight - height) / 2 // derived: center within the first header row
        spacing: Refine.headApart
        Row {
            id: loadingNote
            visible: head.loading !== ""
            anchors.verticalCenter: parent.verticalCenter
            spacing: Refine.innerApart
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.loader2; size: Refine.rowButtonIcon; color: Theme.color.textSecondary }
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trArgs("{} still loading · counts may grow", [head.loading], I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
        }
        Row {
            id: selectionCount
            objectName: "refineCount"
            anchors.verticalCenter: parent.verticalCenter
            Text {
                id: countNumber
                text: head.countWord
                font.pointSize: Type.body
                font.weight: Type.heading
                font.features: { "tnum": 1 }
                color: head.countWord === "0" ? Theme.color.statusNegative : Theme.color.textPrimary
            }
            Text {
                text: head.summary.slice(head.countWord.length)
                width: Math.min(implicitWidth, Math.max(0, head.width - Refine.headLeft - Refine.headRight
                    - left.implicitWidth - actions.width - countNumber.width - Refine.headApart * 3
                    - (loadingNote.visible ? loadingNote.width + Refine.headApart : 0)))
                elide: Text.ElideRight
                font.pointSize: Type.body
                font.features: { "tnum": 1 }
                color: Theme.color.textSecondary
            }
        }
        Row {
            id: actions
            spacing: Refine.headApart
            SquareButton {
                id: saveButton
                objectName: "refineSaveQuery"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Lucide.save
                iconFamily: Lucide.font
                enabled: head.canSave
                quiet: !head.canSave
                tip: I18n.tr("Save query", I18n.rev)
                onClicked: head.saveAsked()
            }
            SquareButton {
                id: savedButton
                objectName: "refineSavedQueries"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Lucide.fileUp
                iconFamily: Lucide.font
                tip: I18n.tr("Saved queries", I18n.rev)
                onClicked: head.savedAsked()
            }
            SquareButton {
                objectName: "refineConsoleToggle"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.terminal2
                checked: head.consoleShown
                tip: head.consoleShown ? I18n.tr("Hide the console line", I18n.rev) : I18n.tr("Show the query as console text", I18n.rev)
                onClicked: head.consoleToggled()
                Rectangle {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: Metrics.hairline
                    width: Refine.eye + Metrics.hairline * 2 // derived: a ring around the glyph
                    height: width
                    radius: width / 2 // derived: a circle
                    color: Theme.color.surfacePane
                    Icon { anchors.centerIn: parent; glyph: head.consoleShown ? Icons.eyeOff : Icons.eye; size: Refine.eye; color: head.consoleShown ? Theme.color.accent : Theme.color.textSecondary }
                }
            }
            SquareButton {
                objectName: "refineUndo"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.arrowBackUp
                quiet: !head.canUndo
                tip: I18n.tr("Undo", I18n.rev)
                onClicked: head.undoAsked()
            }
            Rectangle {
                objectName: "refineClear"
                anchors.verticalCenter: parent.verticalCenter
                width: clear.implicitWidth + Refine.textButtonPad * 2 // derived: padding on both sides
                height: Refine.textButtonTall
                radius: Refine.buttonRound
                color: clearArea.containsMouse ? Theme.color.stateHover : "transparent"
                Text {
                    id: clear
                    anchors.centerIn: parent
                    text: I18n.tr("Clear all", I18n.rev)
                    font.pointSize: Type.row
                    color: head.conditions > 0 ? Theme.color.textSecondary : Theme.color.textTertiary
                }
                MouseArea {
                    id: clearArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: head.clearAsked()
                }
            }
            SquareButton {
                objectName: "refineDetach"
                visible: head.canDetach
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.appWindow
                tip: I18n.tr("Open Refine in its own window", I18n.rev)
                onClicked: head.detachAsked()
            }
        }
    }
}
