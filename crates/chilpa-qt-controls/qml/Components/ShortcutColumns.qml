import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Each group fills a responsive grid of shortcut rows.
Flickable {
    id: view

    // The sheet as the facade writes it: three columns of groups, and what
    // the editor shows of a change of key in progress.
    property var keys: ({})
    property bool editing: false

    function groupsFor(groups) {
        if (editing) return groups
        return groups.map(group => ({label:group.label,
            rows:(group.rows || []).filter(row => (row.shortcut || "") !== "")}))
            .filter(group => group.rows.length > 0)
    }

    implicitHeight: columns.implicitHeight
    contentHeight: columns.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar { id: bar; objectName: "shortcutScrollBar" }

    Column {
        id: columns
        objectName: "shortcutColumns"
        width: Math.max(0, view.width - Metrics.gap24)
        spacing: Metrics.gap16
        Repeater {
            model: view.groupsFor([...(view.keys.col1 || []), ...(view.keys.col2 || []), ...(view.keys.col3 || [])])
            Column {
                id: group
                objectName: "shortcutGroup"
                required property var modelData
                width: columns.width
                spacing: Metrics.gap6
                Text {
                    text: group.modelData.label
                    font.pointSize: Type.meta
                    font.weight: Type.semibold
                    color: Theme.color.textTertiary
                }
                Grid {
                    id: grid
                    width: parent.width
                    columns: Math.max(1, Math.min(3, Math.floor(width / Sizes.shortcutCell)))
                    spacing: Metrics.gap8
                    Repeater {
                        model: group.modelData.rows
                        ShortcutRow {
                            objectName: "shortcutRow"
                            required property var modelData
                            width: (grid.width - grid.spacing * (grid.columns - 1)) / grid.columns
                            entry: modelData
                            editing: view.editing
                            recording: view.editing && view.keys.recording === modelData.id
                            tried: view.keys.tried || ""
                            takenBy: view.keys.takenBy || ""
                            onRecord: Hotkeys.command("record", modelData.id)
                            onReset: Hotkeys.command("reset", modelData.id)
                        }
                    }
                }
            }
        }
    }
}
