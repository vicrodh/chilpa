import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The view picker's menu (03-immersive §1.2): 660 wide on a dark glass,
// FOCUS in two balanced columns and SPLIT in the third, rows of 33 with
// the active one lit and dotted. A row sets the view through the bridge,
// the only mutator; the arrows of the trigger walk one circular sequence
// over FOCUS and then SPLIT. The scopes join when the renderer can draw
// them; the scenes come with their block.
Popup {
    id: menu
    property bool scopes: Immersive.accelerated
    width: Stage.menuWide
    padding: Stage.menuPad
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        radius: Stage.menuRound
        color: Theme.color.immersiveMenu
        border.width: Metrics.hairline
        border.color: Theme.color.immersiveGlassEdge
    }

    function openBelow(item) {
        x = item.x
        y = item.y + item.height + Stage.menuApart
        open()
    }

    readonly property var focusRows: {
        const rows = [
            { viewMode: 0, mode: 0, panel: -1, label: I18n.tr("Album Reactive", I18n.rev) },
            { viewMode: 0, mode: 1, panel: -1, label: I18n.tr("Static", I18n.rev) },
            { viewMode: 0, mode: 2, panel: -1, label: I18n.tr("Coverflow", I18n.rev) },
            { viewMode: 0, mode: 4, panel: -1, label: I18n.tr("Lyrics", I18n.rev) },
            { viewMode: 0, mode: 5, panel: -1, label: I18n.tr("Queue", I18n.rev) },
            { viewMode: 0, mode: 3, panel: -1, label: I18n.tr("Spectrum", I18n.rev) },
            { viewMode: 0, mode: 6, panel: -1, label: I18n.tr("Wave Bed", I18n.rev) },
            { viewMode: 0, mode: 9, panel: -1, label: I18n.tr("Reactive Rings", I18n.rev) }
        ]
        if (menu.scopes) {
            rows.push({ viewMode: 0, mode: 7, panel: -1, label: I18n.tr("Goniometer", I18n.rev) })
            rows.push({ viewMode: 0, mode: 8, panel: -1, label: I18n.tr("Oscilloscope", I18n.rev) })
            for (const row of [
                [1, I18n.tr("Plasma", I18n.rev)], [2, I18n.tr("Tunnel", I18n.rev)], [3, I18n.tr("Aurora", I18n.rev)], [7, I18n.tr("Ambient", I18n.rev)],
                [4, I18n.tr("Spectral Ribbon", I18n.rev)], [5, I18n.tr("Line Bed", I18n.rev)], [8, I18n.tr("Tunnel Flow", I18n.rev)]
            ])
                rows.push({ viewMode: 0, scene: row[0], label: row[1] })
        }
        return rows
    }
    readonly property var splitRows: [
        { viewMode: 1, mode: -1, panel: 0, label: I18n.tr("Lyrics", I18n.rev) },
        { viewMode: 1, mode: -1, panel: 3, label: I18n.tr("Queue", I18n.rev) },
        { viewMode: 1, mode: -1, panel: 4, label: I18n.tr("Now Playing", I18n.rev) },
        { viewMode: 1, mode: -1, panel: 5, label: I18n.tr("Now Playing", I18n.rev) + " · " + I18n.tr("Lyrics", I18n.rev) }
    ]
    readonly property int focusCut: Math.ceil(focusRows.length / 2) // derived: two balanced columns

    function rowIsActive(row) {
        if (row.scene !== undefined)
            return Immersive.scene === row.scene
        if (row.viewMode === 0)
            return Immersive.viewMode === 0 && Immersive.mode === row.mode && Immersive.scene === 0
        return Immersive.viewMode === 1 && Immersive.splitPanel === row.panel && Immersive.scene === 0
    }
    function fireRow(row) {
        if (row.scene !== undefined) {
            Immersive.setView(0, Immersive.mode, Immersive.splitPanel)
            Immersive.chooseScene(row.scene)
            menu.close()
            return
        }
        // A panel row turns any scene off first.
        if (Immersive.scene !== 0)
            Immersive.chooseScene(0)
        Immersive.setView(row.viewMode, row.viewMode === 0 ? row.mode : Immersive.mode, row.viewMode === 0 ? Immersive.splitPanel : row.panel)
        menu.close()
    }
    function cycle(direction) {
        const rows = focusRows.concat(splitRows)
        let current = -1
        for (let i = 0; i < rows.length; ++i) {
            if (rowIsActive(rows[i])) {
                current = i
                break
            }
        }
        const next = current < 0 ? (direction < 0 ? rows.length - 1 : 0) : (current + direction + rows.length) % rows.length
        fireRow(rows[next])
    }

    component MenuRow: Rectangle {
        id: row
        required property var modelData
        readonly property bool active: menu.rowIsActive(modelData)
        width: parent ? parent.width : 0
        height: Stage.menuRowTall
        radius: Stage.menuRowRound
        color: active ? Theme.color.immersiveRowActive : (area.containsMouse ? Theme.color.immersiveRowHover : "transparent")
        Text {
            anchors.left: parent.left
            anchors.leftMargin: Stage.menuRowLead
            anchors.right: dot.left
            anchors.rightMargin: Stage.menuDotPad
            height: parent.height
            text: row.modelData.label
            color: Theme.color.immersiveText
            font.pointSize: Stage.menuRowText
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Rectangle {
            id: dot
            visible: row.active
            anchors.right: parent.right
            anchors.rightMargin: Stage.menuDotIn
            anchors.verticalCenter: parent.verticalCenter
            width: Stage.menuDot
            height: Stage.menuDot
            radius: width / 2 // derived: a dot
            color: Theme.color.immersiveTextStrong
        }
        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: menu.fireRow(row.modelData)
        }
    }
    component Heading: Text {
        height: Stage.menuHeadTall
        leftPadding: Stage.menuRowLead
        verticalAlignment: Text.AlignVCenter
        color: Theme.color.immersiveTextGhost
        font.pointSize: Stage.menuHeadText
        font.weight: Type.bold
        font.letterSpacing: Stage.menuHeadTracking
    }

    contentItem: Column {
        spacing: 0
        Row {
            width: parent.width
            spacing: Stage.menuApart
            Heading { width: (parent.width - Stage.menuApart * 2) / 3 * 2 + Stage.menuApart; text: I18n.tr("FOCUS", I18n.rev) } // derived: two columns and the gap between them
            Heading { width: (parent.width - Stage.menuApart * 2) / 3; text: I18n.tr("SPLIT", I18n.rev) } // derived: the third column
        }
        Row {
            id: columns
            width: parent.width
            spacing: Stage.menuApart
            readonly property real columnWide: (width - spacing * 2) / 3 // derived: three columns
            Column { width: columns.columnWide; spacing: Stage.menuRowApart; Repeater { model: menu.focusRows.slice(0, menu.focusCut); delegate: MenuRow {} } }
            Column { width: columns.columnWide; spacing: Stage.menuRowApart; Repeater { model: menu.focusRows.slice(menu.focusCut); delegate: MenuRow {} } }
            Column { width: columns.columnWide; spacing: Stage.menuRowApart; Repeater { model: menu.splitRows; delegate: MenuRow {} } }
        }
    }
}
