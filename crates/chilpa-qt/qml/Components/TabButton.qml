import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A tab: icon, title and its own close button (design/02 3). A middle click
// closes it too. A tab that can be renamed is renamed in place: the pencil
// on the active one, or a double click on its name, turns the name into a
// field; Enter or leaving it keeps the name, Escape keeps the old one.
Item {
    id: tab

    property string title
    property string glyph
    property string sourceIcon
    property bool active: false
    property bool renamable: false
    // What renaming starts from: the name alone, without what the title adds.
    property string name: title
    property bool editing: false

    signal activated()
    signal closed()
    signal renamed(string name)

    function rename() {
        if (!renamable || editing)
            return
        field.text = name
        editing = true
        field.forceActiveFocus()
        field.selectAll()
    }
    function finish(keep) {
        if (!editing)
            return
        editing = false
        const said = field.text.trim()
        if (keep && said !== "" && said !== name)
            tab.renamed(said)
    }

    implicitWidth: Math.min(Metrics.tabMaxWidth, row.implicitWidth + Metrics.tabPadLeft + Metrics.tabPadRight)
    implicitHeight: Metrics.tabHeight
    RectangularShadow {
        anchors.fill: panel
        visible: tab.active
        radius: panel.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowTabActive
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: tab.active ? (Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfaceRaised) : area.containsMouse ? Theme.color.stateHover : "transparent"

        Edge { visible: tab.active }
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        onClicked: (mouse) => mouse.button === Qt.MiddleButton ? tab.closed() : tab.activated()
        onDoubleClicked: (mouse) => { if (mouse.button === Qt.LeftButton) tab.rename() }
    }

    Row {
        id: row
        anchors.left: parent.left
        anchors.leftMargin: Metrics.tabPadLeft
        anchors.right: parent.right
        anchors.rightMargin: Metrics.tabPadRight
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: tab.sourceIcon === ""
            glyph: tab.glyph
            size: Metrics.iconSm
            color: Theme.color.textTertiary
        }
        SourceIcon {
            anchors.verticalCenter: parent.verticalCenter
            visible: tab.sourceIcon !== ""
            sourceInfo: ({icon: tab.sourceIcon})
            width: visible ? Metrics.iconSm : 0
            height: Metrics.iconSm
            color: Theme.color.textTertiary
        }

        Text {
            id: label
            visible: !tab.editing
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Metrics.tabMaxWidth - Metrics.tabPadLeft - Metrics.tabPadRight - Metrics.iconSm - Metrics.tabClose - Metrics.gap12)
            text: tab.title
            elide: Text.ElideRight
            font.pointSize: Type.meta
            font.weight: tab.active ? Type.semibold : Type.regular
            color: tab.active ? Theme.color.textPrimary : Theme.color.textSecondary
        }

        TextInput {
            id: field
            objectName: "tabName"
            visible: tab.editing
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(label.width, Metrics.tabRenameMin)
            clip: true
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.textPrimary
            selectionColor: Theme.color.rowSelected
            onAccepted: tab.finish(true)
            onActiveFocusChanged: if (!activeFocus) tab.finish(true)
            Keys.onEscapePressed: tab.finish(false)
        }

        IconButton {
            objectName: "renameTab"
            visible: tab.renamable && tab.active && !tab.editing
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Rename", I18n.rev)
            glyph: Icons.pencil
            size: Metrics.tabClose
            iconSize: Metrics.iconXs
            onClicked: tab.rename()
        }

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Close tab", I18n.rev)
            glyph: Icons.x
            size: Metrics.tabClose
            iconSize: Metrics.iconXs
            onClicked: tab.closed()
        }
    }
}
