import QtQuick
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Every shortcut, by group, in three columns that fill one after another.
// It scrolls when the window is too short for it.
Flickable {
    id: view

    // The sheet as the facade writes it: three columns of groups, and what
    // the editor shows of a change of key in progress.
    property var keys: ({})
    property bool editing: false

    implicitHeight: columns.implicitHeight
    contentHeight: columns.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    Row {
        id: columns
        width: view.width
        spacing: Metrics.gap24

        Repeater {
            model: [view.keys.col1 || [], view.keys.col2 || [], view.keys.col3 || []]

            Column {
                id: column
                required property var modelData
                width: (columns.width - columns.spacing * 2) / 3 // derived: three alike
                spacing: Metrics.gap16

                Repeater {
                    model: column.modelData

                    Column {
                        id: group
                        required property var modelData
                        width: column.width
                        spacing: Metrics.gap6

                        Text {
                            text: group.modelData.label
                            font.pointSize: Type.meta
                            font.weight: Type.semibold
                            color: Theme.color.textTertiary
                        }
                        Rectangle {
                            visible: !view.editing
                            width: parent.width
                            height: 1
                            color: Theme.color.strokeHairline
                        }
                        Repeater {
                            model: group.modelData.rows

                            ShortcutRow {
                                required property var modelData
                                width: group.width
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
}
