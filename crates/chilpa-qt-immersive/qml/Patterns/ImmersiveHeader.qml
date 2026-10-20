import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

// The band over the immersive view (03-immersive §1.2), 36 tall between
// 24 px margins: the view picker at the left with its menu, the search
// field centered at min(width, 420) with the Enter hint while results
// show, and the window capsule at the right. The band itself drags the
// window and a double click maximizes it, unless it is full screen. The
// host fades the band with the chrome.
Item {
    id: header
    property Window hostWindow: null
    property bool resultsOpen: false
    readonly property bool typing: field.editing
    property alias query: field.text
    signal stepAsked(int by)
    signal acceptAsked()
    signal escapeAsked()
    signal exitAsked()
    function focusSearch() { field.focusInput() }
    function releaseSearch() { field.release() }
    function cycle(by) { menu.cycle(by) }
    // On a Mac the band's left corner is the system's window controls, so
    // the view picker sits at the right, beside the capsule, keeping the
    // room the capsule takes open so it never slides when that one grows
    // (QBZ, ImmersiveHeader.qml:76-98). Elsewhere it stays at the left.
    property bool pickerTrailing: Qt.platform.os === "osx"

    MouseArea {
        anchors.fill: parent
        enabled: header.hostWindow !== null && header.hostWindow.visibility !== Window.FullScreen
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.ArrowCursor
        property bool dragging: false
        onPressed: dragging = false
        onReleased: dragging = false
        onPositionChanged: {
            if (pressed && !dragging && header.hostWindow) {
                dragging = true
                header.hostWindow.startSystemMove()
            }
        }
        onDoubleClicked: {
            if (header.hostWindow)
                header.hostWindow.visibility = header.hostWindow.visibility === Window.Maximized ? Window.Windowed : Window.Maximized
        }
    }

    ViewTrigger {
        id: trigger
        objectName: "immersiveViewTrigger"
        x: header.pickerTrailing ? header.width - capsule.openWide - Stage.triggerApart - width : 0
        anchors.verticalCenter: parent.verticalCenter
        onPrevious: menu.cycle(-1)
        onNext: menu.cycle(1)
        onMenu: menu.openBelow(trigger)
    }
    ViewMenu { id: menu }

    SearchField {
        id: field
        objectName: "immersiveSearch"
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(header.width, Stage.searchWide)
        placeholder: I18n.tr("Search", I18n.rev)
        onStepped: (by) => header.stepAsked(by)
        onAccepted: header.acceptAsked()
        onEscaped: header.escapeAsked()
    }
    Text {
        visible: header.resultsOpen
        anchors.left: field.right
        anchors.leftMargin: Stage.hintApart
        anchors.verticalCenter: field.verticalCenter
        text: "↵  " + I18n.tr("Enter", I18n.rev)
        color: Theme.color.immersiveTextDim
        font.pointSize: Stage.hintText
    }

    WindowCapsule {
        id: capsule
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        hostWindow: header.hostWindow
        onExit: header.exitAsked()
    }
}
