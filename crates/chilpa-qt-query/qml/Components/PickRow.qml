import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One value to add (F3): its state (an empty circle, a check in the accent
// once included, a minus once kept out), the value and a quieter hint, how
// many it would leave, and + and − always there. A click on the row
// includes; − keeps it out. Lit while it is in the query.
Rectangle {
    id: line
    property string text
    property string hint: ""
    property string count: ""
    // "none", "include" or "exclude".
    property string mark: "none"
    // A glyph in place of the state circle, as the words rows have.
    property string glyph: ""
    property bool italic: false
    property bool actions: true
    // Words can be added but not kept out: the query has no negated words.
    property bool canExclude: true
    signal included()
    signal excluded()

    implicitHeight: Refine.pickRow
    radius: Refine.pickRowRound
    color: area.containsMouse || mark !== "none" ? Theme.color.stateHover : "transparent"

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: line.included()
    }
    Icon {
        x: Refine.pickPad
        anchors.verticalCenter: parent.verticalCenter
        glyph: line.glyph !== "" ? line.glyph : line.mark === "include" ? Icons.circleCheckFilled : line.mark === "exclude" ? Icons.circleMinus : Icons.circle
        filled: line.glyph === "" && line.mark === "include"
        size: Refine.pickStateIcon
        color: line.glyph === "" && line.mark === "include" ? Theme.color.accent : line.glyph === "" && line.mark === "exclude" ? Theme.color.textPrimary : Theme.color.textTertiary
    }
    Row {
        x: Refine.pickPad + Refine.pickState + Refine.pickPad
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - x - counted.width - actionsRow.width - Refine.pickPad * 2 // derived: room left of the count and the buttons
        spacing: Refine.innerApart
        clip: true
        Text {
            text: line.text
            font.pointSize: Type.body
            font.italic: line.italic
            color: line.italic ? Theme.color.textSecondary : line.mark === "exclude" ? Theme.color.textSecondary : Theme.color.textPrimary
            elide: Text.ElideRight
        }
        Text {
            visible: line.hint !== ""
            anchors.baseline: parent.children[0].baseline
            text: line.hint
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
    }
    Text {
        id: counted
        anchors.right: actionsRow.left
        anchors.rightMargin: Refine.pickPad
        anchors.verticalCenter: parent.verticalCenter
        text: line.count
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    Row {
        id: actionsRow
        anchors.right: parent.right
        anchors.rightMargin: Refine.rowPadRight
        anchors.verticalCenter: parent.verticalCenter
        width: Refine.pickActions
        layoutDirection: Qt.RightToLeft
        spacing: Refine.segTrackPad
        visible: line.actions
        component Action: Rectangle {
            id: action
            property string glyph
            property bool on: false
            property string tip
            signal clicked()
            width: Refine.pickAction
            height: Refine.pickActionTall
            radius: Refine.pickRowRound
            color: on ? Theme.color.stateSelected : hover.containsMouse ? Theme.color.stateSelected : "transparent"
            Icon { anchors.centerIn: parent; glyph: action.glyph; size: Refine.rowButtonIcon; color: action.on ? (action.glyph === Icons.plus ? Theme.color.accent : Theme.color.textPrimary) : Theme.color.textSecondary }
            MouseArea {
                id: hover
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: action.clicked()
                onContainsMouseChanged: containsMouse ? TipState.show(action.tip, action) : TipState.hide(action)
            }
            Component.onDestruction: TipState.hide(action)
        }
        Action { objectName: "pickExclude"; visible: line.canExclude; glyph: Icons.minus; on: line.mark === "exclude"; tip: I18n.tr("Exclude", I18n.rev); onClicked: line.excluded() }
        Action { objectName: "pickInclude"; glyph: Icons.plus; on: line.mark === "include"; tip: I18n.tr("Include", I18n.rev); onClicked: line.included() }
    }
}
