import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One row of Settings (design/03 SettingsRow): a group's title, a setting
// with its control on the right, or a line of a report under one. The
// control comes from the setting's kind in the schema; no setting has QML
// of its own (frontend ADR-0010).
Item {
    id: row

    required property int rowKind
    required property string key
    required property string title
    required property string description
    required property string control
    required property string action
    required property string value
    required property var options
    required property var optionValues
    required property var marks

    // The page, or what stands in front of it to add a step (SettingsScreen).
    property var page

    // A group's title, or a line of a report under a setting.
    readonly property bool group: rowKind === 0
    readonly property bool line: rowKind === 2

    // "fixed": what the output allows, shown and not changeable; the whole
    // row dims and the description says why (design/03 SettingsRow).
    opacity: control === "fixed" ? Metrics.disabledOpacity : 1
    implicitHeight: group ? Metrics.settingsGroup
        : Math.max(line ? Metrics.menuItem : Metrics.settingsRow, texts.implicitHeight + Metrics.gap12)

    Text {
        visible: row.group
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap6
        text: row.title.toUpperCase()
        font.pointSize: Type.label
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
        color: Theme.color.textTertiary
    }

    Column {
        id: texts
        visible: !row.group
        anchors.left: parent.left
        anchors.leftMargin: row.line ? Metrics.gap16 : 0
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.title
            wrapMode: Text.Wrap
            font.pointSize: row.line ? Type.row : Type.bodyLarge
            font.weight: row.line ? Type.regular : Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            visible: row.description !== ""
            text: row.description
            wrapMode: Text.Wrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    // Only the row's own control is built.
    Loader {
        id: controls
        visible: !row.group
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        sourceComponent: row.line ? verdict
            : row.control === "toggle" || row.control === "fixed" ? toggle
            : row.control === "select" || row.control === "device" ? select
            : row.control === "segmented" ? segmented
            : row.control === "button" || row.control === "link" ? button
            : row.control === "secret" ? secret
            : row.control === "external" ? external
            : row.control === "swatches" ? swatches
            : row.control === "progress" ? progress
            : row.control === "status" ? status
            : null
    }

    Component {
        id: toggle
        Toggle {
            enabled: row.control === "toggle"
            checked: row.value === "true"
            onToggled: (on) => row.page.set(row.key, on ? "true" : "false")
        }
    }
    Component {
        id: select
        SelectField {
            options: row.options
            values: row.optionValues
            marks: row.marks
            value: row.value
            onPicked: (value) => row.page.set(row.key, value)
        }
    }
    Component {
        id: segmented
        SegmentedControl {
            options: row.options.map((text) => ({ text: text }))
            current: row.optionValues.indexOf(row.value)
            onActivated: (index) => row.page.set(row.key, row.optionValues[index])
        }
    }
    Component {
        id: button
        PillButton {
            text: row.action
            glyph: row.control === "link" ? Icons.chevronRight : ""
            onClicked: row.page.press(row.key)
        }
    }
    Component {
        id: secret
        Row {
            spacing: Metrics.gap8
            TextField {
                id: field
                anchors.verticalCenter: parent.verticalCenter
                secret: true
                placeholder: I18n.tr("Token", I18n.rev)
                onAccepted: if (text.trim() !== "") send.clicked()
            }
            PillButton {
                id: send
                anchors.verticalCenter: parent.verticalCenter
                text: row.action
                enabled: field.text.trim() !== ""
                onClicked: row.page.set(row.key, field.text)
            }
        }
    }
    // Opens the page in the browser, once when it appears -- the person
    // just asked to connect -- and again on a click.
    Component {
        id: external
        PillButton {
            text: row.action
            glyph: Icons.arrowUpRight
            onClicked: Qt.openUrlExternally(row.value)
            Component.onCompleted: Qt.openUrlExternally(row.value)
        }
    }
    Component {
        id: swatches
        Swatches {
            colors: row.options
            values: row.optionValues
            value: row.value
            onPicked: (value) => row.page.set(row.key, value)
        }
    }
    Component {
        id: progress
        Row {
            spacing: Metrics.gap12

            Rectangle {
                visible: row.value !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.settingsNav
                height: Metrics.progress
                radius: height
                color: Theme.color.strokeHairline

                Rectangle {
                    width: parent.width * Math.max(0, Math.min(1, Number(row.value)))
                    height: parent.height
                    radius: parent.radius
                    color: Theme.color.accent
                }
            }
            PillButton {
                anchors.verticalCenter: parent.verticalCenter
                text: row.action
                onClicked: row.page.press(row.key)
            }
        }
    }
    Component {
        id: status
        StatusField {
            state: row.action
            link: row.value
            openText: I18n.tr("Open account page", I18n.rev)
            options: row.options
            values: row.optionValues
            onPicked: (value) => row.page.set(row.key, value)
        }
    }
    Component {
        id: verdict
        Text {
            text: row.value
            font.pointSize: Type.row
            color: row.value === "bit-perfect" ? Theme.color.accent : Theme.color.textSecondary
        }
    }
}
