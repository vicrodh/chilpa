import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
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
    property var page
    property Component serverEditor
    // The theme's picker, and the options of the auto or custom theme under it.
    property Component themePicker
    property Component themeOptions
    readonly property bool themeBox: control === "theme" && (value === "auto" || value === "custom")
    readonly property bool group: rowKind === 0
    readonly property bool line: rowKind === 2
    readonly property bool editingServer: page && page.editingServerKey === key && key.startsWith("sources.server/")
    readonly property real baseHeight: group ? Metrics.settingsGroup : Math.max(line ? Metrics.menuItem : Metrics.settingsRow, texts.implicitHeight + Metrics.gap12)
    opacity: control === "fixed" ? Metrics.disabledOpacity : 1
    implicitHeight: baseHeight + (editingServer ? editor.implicitHeight + Metrics.gap12 : 0) + (themeBox ? options.implicitHeight + Metrics.gap12 : 0)
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
    SettingText {
        id: texts
        visible: !row.group
        anchors.left: parent.left
        anchors.leftMargin: row.line ? Metrics.gap16 : 0
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap16
        y: (row.baseHeight - height) / 2
        title: row.title
        description: row.description
        line: row.line
    }
    // Only the row's own control is built.
    Loader {
        id: controls
        visible: !row.group
        anchors.right: parent.right
        y: (row.baseHeight - height) / 2
        sourceComponent: row.line ? verdict
            : row.control === "toggle" || (row.control === "fixed" && row.options.length === 0) ? toggle
            : row.control === "select" || row.control === "device" || row.control === "fixed" ? select
            : row.control === "segmented" ? segmented
            : row.control === "button" || row.control === "link" ? button
            : row.control === "secret" ? secret
            : row.control === "external" ? external
            : row.control === "swatches" ? swatches
            : row.control === "progress" ? progress
            : row.control === "status" ? status
            : row.control === "slider" ? slider
            : row.control === "file" ? file
            : row.control === "theme" ? row.themePicker
            : null
    }

    Loader { id: editor; active: row.editingServer; width: parent.width; y: row.baseHeight + Metrics.gap12
        sourceComponent: row.serverEditor }
    Loader { id: options; active: row.themeBox; width: parent.width; y: row.baseHeight + Metrics.gap12
        sourceComponent: row.themeOptions }

    Component {
        id: toggle
        Toggle {
            enabled: row.control === "toggle"
            checked: row.value === "true"
            onToggled: (on) => row.page.set(row.key === "navigation.extraControls" ? "playback.extra"
                : row.key === "navigation.autoHide" ? "playback.autoHide" : row.key, on ? "true" : "false")
        }
    }
    Component {
        id: select
        SelectField {
            enabled: row.control !== "fixed"
            options: row.options
            values: row.optionValues
            marks: row.marks
            value: row.key === "navigation.playerLayout" ? PlayerLayout.mode : row.value
            onPicked: (value) => {
                if (row.key === "navigation.playerLayout") PlayerLayout.mode = value
                else row.page.set(row.key, value)
            }
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
            onClicked: row.key === "storage.open" ? Qt.openUrlExternally(row.value) : row.page.press(row.key)
        }
    }
    Component {
        id: secret
        SecretField {
            placeholder: I18n.tr("Token", I18n.rev)
            action: row.action
            onSent: (text) => row.page.set(row.key, text)
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
        ProgressField {
            progress: row.value
            action: row.action
            onAsked: row.page.press(row.key)
        }
    }
    Component {
        id: status
        StatusField {
            state: row.action
            light: row.marks.length > 0 ? row.marks[0] : ""
            link: row.value
            openText: I18n.tr("Open account page", I18n.rev)
            options: row.options
            values: row.optionValues
            onPicked: (value) => row.page.set(row.key, value)
        }
    }
    // A level from 0 to 100: whoever shows it follows the drag through
    // Prefs.preview, and the value is kept when the pointer lets go.
    Component {
        id: slider
        LevelField {
            value: Number(row.value)
            shown: Prefs.previewKey === row.key ? Prefs.previewValue : row.value
            tip: row.title
            onMoved: (level) => Prefs.preview(row.key, String(level))
            onReleased: (level) => {
                Prefs.preview("", "")
                row.page.set(row.key, String(level))
            }
        }
    }
    // A file chosen from a dialog; with one chosen, a way back to none.
    Component {
        id: file
        FileField {
            action: row.action
            value: row.value
            title: row.title
            clearTip: I18n.tr("Use the default", I18n.rev)
            filters: [I18n.tr("Images", I18n.rev) + " (*.png *.jpg *.jpeg *.webp *.bmp)"]
            onPicked: (path) => row.page.set(row.key, path)
            onCleared: row.page.set(row.key, "")
        }
    }
    Component { id: verdict; Text {
        text: row.value; font.pointSize: Type.row
        color: row.value === "bit-perfect" ? Theme.color.accent : Theme.color.textSecondary
    } }
}
