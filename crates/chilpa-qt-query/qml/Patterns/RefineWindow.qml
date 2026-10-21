import QtQuick
import QtQuick.Window
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// Refine in a window of its own (F9), for a second screen: a real top-level
// window with its own 34 px bar (the name, how many tracks, fold to the bar,
// dock back). Dragging the bar moves it; a double click or the fold button
// folds it to its bar and back; the desktop's own minimize still applies.
// It lives only while Refine is out: docking destroys it.
Window {
    id: window
    property string summary: ""
    property bool folded: false
    signal dockAsked()

    readonly property string countWord: summary.split(" ")[0]
    title: I18n.tr("Refine", I18n.rev)
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"
    width: Refine.windowWide
    height: folded ? Refine.windowBar : Math.min(Screen.desktopAvailableHeight - Refine.windowRoom, Refine.windowBar + panel.implicitHeight)
    minimumWidth: Refine.windowWide / 2 // derived: half its width at least
    visible: true

    Rectangle {
        anchors.fill: parent
        radius: Refine.panelRound
        color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfacePane
        border.width: Metrics.hairline
        border.color: Theme.color.strokeHairline
        clip: true

        Item {
            id: bar
            objectName: "refineWindowBar"
            width: parent.width
            height: Refine.windowBar
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.SizeAllCursor
                onPressed: window.startSystemMove()
                onDoubleClicked: window.folded = !window.folded
            }
            Row {
                x: Refine.footSide
                anchors.verticalCenter: parent.verticalCenter
                spacing: Refine.headApart
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.adjustmentsHorizontal; size: Refine.labelIcon; color: Theme.color.textSecondary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Refine", I18n.rev); font.pointSize: Type.row; font.weight: Type.heading; color: Theme.color.textPrimary }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "· " + window.summary.split(" · ")[0]; font.pointSize: Type.meta; font.features: { "tnum": 1 }; color: Theme.color.textSecondary }
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: Refine.innerApart
                anchors.verticalCenter: parent.verticalCenter
                SquareButton { objectName: "refineFold"; width: Refine.windowButton; height: Refine.windowButton; round: Refine.windowButtonRound; glyph: window.folded ? Icons.chevronDown : Icons.minus; tip: window.folded ? I18n.tr("Restore", I18n.rev) : I18n.tr("Minimize", I18n.rev); onClicked: window.folded = !window.folded }
                SquareButton { objectName: "refineDock"; width: Refine.windowButton; height: Refine.windowButton; round: Refine.windowButtonRound; glyph: Icons.layoutNavbarInactive; tip: I18n.tr("Dock back into the Library", I18n.rev); onClicked: window.dockAsked() }
            }
            Rectangle { visible: !window.folded; anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
        }
        RefinePanel {
            id: panel
            visible: !window.folded
            anchors.top: bar.bottom
            width: parent.width
            height: parent.height - bar.height
            bare: true
            detached: true
            summary: window.summary
        }
    }
}
