import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Settings > Navigation > Sidebar entries: every entry with its switch and
// its place. Each change sends the whole layout; the facade keeps it and
// refuses one with nothing shown.
Column {
    id: choices
    property string value: ""
    signal changed(string value)
    spacing: Metrics.gap4
    LibraryDestinations { id: destinations; layout: choices.value }
    readonly property var rows: destinations.choices
    readonly property int shown: rows.filter(row => row.on).length

    function send(rows) { changed(rows.map(row => (row.on ? "" : "-") + row.screen).join(",")) }
    function move(index, offset) {
        const next = rows.slice(), to = index + offset
        if (to < 0 || to >= next.length) return
        next.splice(to, 0, next.splice(index, 1)[0])
        send(next)
    }
    function toggle(index, on) {
        send(rows.map((row, at) => at === index ? Object.assign({}, row, { on: on }) : row))
    }

    Repeater {
        model: choices.rows
        Item {
            id: entry
            required property var modelData
            required property int index
            width: choices.width
            height: Metrics.button28
            Icon {
                id: glyph
                anchors.verticalCenter: parent.verticalCenter
                glyph: entry.modelData.glyph
                iconFamily: entry.modelData.family || ""
                color: Theme.color.textSecondary
            }
            Text {
                anchors.left: glyph.right
                anchors.leftMargin: Metrics.gap8
                anchors.right: moves.left
                anchors.verticalCenter: parent.verticalCenter
                text: entry.modelData.text
                elide: Text.ElideRight
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Row {
                id: moves
                anchors.right: switcher.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                IconButton {
                    objectName: "sidebarEntryUp-" + entry.modelData.screen
                    glyph: Icons.chevronUp
                    tip: I18n.tr("Move up", I18n.rev)
                    size: Metrics.button24
                    enabled: entry.index > 0
                    onClicked: choices.move(entry.index, -1)
                }
                IconButton {
                    objectName: "sidebarEntryDown-" + entry.modelData.screen
                    glyph: Icons.chevronDown
                    tip: I18n.tr("Move down", I18n.rev)
                    size: Metrics.button24
                    enabled: entry.index < choices.rows.length - 1
                    onClicked: choices.move(entry.index, 1)
                }
            }
            Toggle {
                id: switcher
                objectName: "sidebarEntryToggle-" + entry.modelData.screen
                compact: true
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: entry.modelData.on
                enabled: !entry.modelData.on || choices.shown > 1
                onToggled: (on) => choices.toggle(entry.index, on)
            }
        }
    }
}
