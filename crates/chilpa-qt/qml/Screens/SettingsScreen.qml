import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
// Qt 6.10 brings a SearchField of its own with QtQuick.Controls; this is the kit's.
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as Driving
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as SourceParts
import "qrc:/qt/qml/app/chilpa/sources/qml/Components/SettingsActions.js" as SettingsActions
import "qrc:/qt/qml/app/chilpa/wizard/qml/Patterns" as WizardPatterns
import "qrc:/qt/qml/app/chilpa/stats/qml/Patterns" as AnalysisUi
import "qrc:/qt/qml/app/chilpa/themes/qml/Patterns" as Themes
// Settings rows follow the facade schema.
Item {
    id: page
    property Component purchaseCard: null
    property string routeSection: ""
    property bool leaving: false
    readonly property string section: routeSection !== "" ? routeSection : SettingsModel.sections[0]
    onLeavingChanged: if (leaving && Navigation.section !== "sources" && actions.editingServerKey !== "") { Servers.cancel(); actions.editingServerKey = "" }
    property alias focusKey: seeking.key
    property alias focusRow: seeking.row
    function go(setting) { search.text = ""; seeking.go(setting) }
    Driving.SettingsFocus { id: seeking; section: page.section; leaving: page.leaving; viewport: list; model: rows; highlight: flash; onLocated: fade.restart() }
    SettingsPage {
        id: rows
        section: page.section
    }
    Connections { target: I18n; function onRevChanged() { rows.refresh() } }
    Text {
        id: title
        x: Metrics.contentX
        y: Metrics.contentTop
        text: I18n.tr("Settings", I18n.rev)
        font.pointSize: Type.title
        font.weight: Type.bold
        font.letterSpacing: Type.titleTracking
        color: Theme.color.textPrimary
    }
    Kit.SearchField {
        id: search
        objectName: "settingsSearch"
        x: Metrics.contentX
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        width: Metrics.settingsNav
        placeholder: I18n.tr("Search settings", I18n.rev)
        onStepped: (by) => found.step(by)
        onAccepted: found.choose()
    }
    Column {
        id: nav
        x: Metrics.contentX
        anchors.top: search.bottom
        anchors.topMargin: Metrics.gap12
        width: Metrics.settingsNav
        Repeater {
            model: SettingsModel.sections
            NavItem {
                required property int index
                required property string modelData
                width: nav.width
                text: I18n.tr(SettingsModel.sectionLabels[index], I18n.rev)
                glyph: Icons[SettingsModel.sectionIcons[index]] || ""
                active: modelData === page.section
                onClicked: Navigation.openSection(modelData)
            }
        }
        PluginSettingsNavigation { width: nav.width }
    }
    // The settings that say what was typed, as the facade orders them:
    // each with where it lives, and a click, or Enter on the one marked,
    // goes there.
    ListView {
        id: found
        visible: search.text !== ""
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        anchors.bottom: parent.bottom
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        width: page.width - x - Metrics.contentX
        property int current: 0
        readonly property var matches: {
            try {
                return JSON.parse(SettingsModel.find(search.text, I18n.rev))
            } catch (e) {
                return []
            }
        }
        signal chosen(var setting)
        function step(by) {
            current = Math.max(0, Math.min(matches.length - 1, current + by))
            positionViewAtIndex(current, ListView.Contain)
        }
        function choose() {
            if (matches.length > 0)
                chosen(matches[current])
        }
        onChosen: (setting) => page.go(setting)
        onMatchesChanged: current = 0
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: matches
        spacing: Metrics.gap2
        ScrollBar.vertical: ViewportScrollBar {}
        delegate: Rectangle {
            required property var modelData
            required property int index
            objectName: "settingFound"
            width: ListView.view.width - Metrics.gap12
            height: lines.implicitHeight + Metrics.gap12 * 2 // derived: padding above and below
            radius: Metrics.radiusControl
            color: index === found.current || hover.hovered ? Theme.color.stateHover : "transparent"
            Column {
                id: lines
                x: Metrics.gap12
                y: Metrics.gap12
                width: parent.width - Metrics.gap12 * 2 // derived: padding on both sides
                spacing: Metrics.gap2
                Text { width: parent.width; text: modelData.title; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
                Text { width: parent.width; text: modelData.place; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textTertiary }
                Text { width: parent.width; visible: text !== ""; text: modelData.description; wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
            }
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: found.chosen(modelData) }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: found.count === 0
            text: I18n.tr("No setting says that", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textTertiary
        }
    }
    ListView {
        id: list
        visible: page.section !== "plugins" && search.text === ""
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section - Metrics.settingsGroup + Metrics.gap12
        anchors.bottom: parent.bottom
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        // To the right edge, whatever the window's width: every row's control
        // sits on the right, its label and description on the left.
        width: page.width - x - Metrics.contentX
        clip: true
        model: rows
        bottomMargin: Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
        delegate: SettingsRow {
            id: settingRow; width: ListView.view.width - Metrics.gap12; page: actions
            serverEditor: Component { SourceParts.ServerEditForm { onClosed: actions.set(settingRow.key, "close-edit") } }
            themePicker: Component { Themes.ThemePicker { options: settingRow.options; values: settingRow.optionValues; value: settingRow.value; onPicked: (slug) => actions.set(settingRow.key, slug) } }
            themeOptions: Component { Themes.ThemeOptionsPanel {} }
        }
    }

    // The row a search went to, lit up behind it for a moment.
    Rectangle {
        id: flash
        property Item target: null
        parent: list.contentItem
        z: -1
        y: target ? target.y : 0
        width: list.width
        height: target ? target.height : 0
        radius: Metrics.radiusControl
        color: Theme.color.rowSelected
        opacity: 0
        NumberAnimation on opacity { id: fade; running: false; from: 1; to: 0; duration: Motion.arrival; easing.type: Easing.InQuad }
    }

    PluginManager {
        id: pluginManager
        visible: page.section === "plugins" && search.text === ""
        page: rows
        purchaseCard: page.purchaseCard
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        anchors.bottom: parent.bottom
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.rightMargin: Metrics.contentX
    }

    QtObject {
        id: actions
        property string editingServerKey: ""
        function set(key, value) { SettingsActions.set(actions, rows, removing, Servers, I18n, key, value) }
        function press(key) { if (!storage.press(key)) SettingsActions.press(actions, rows, pluginManager, casting, adding, diagnosis, inventory, Navigation, Servers, Qt, key) }
    }

    CastPicker { id: casting }

    ServerSetupDialog {
        id: adding
        onClosed: rows.refresh()
    }

    ConfirmDialog {
        id: removing
        property int serverId
        title: I18n.tr("Remove this server?", I18n.rev)
        confirmText: I18n.tr("Remove", I18n.rev)
        destructive: true
        onConfirmed: {
            Servers.remove(serverId); Servers.cancel(); actions.editingServerKey = ""; rows.refresh()
        }
    }

    WizardPatterns.OutputDiagnosisDialog { id: diagnosis; settingsPage: rows }
    AnalysisUi.AnalysisInventoryDialog { id: inventory }
    StorageSettingsActions { id: storage; rows: rows; current: page.visible && page.section === "storage" }

    // A sync moving, a server coming back: its rows follow while shown.
    Timer {
        interval: Motion.refresh
        repeat: true
        running: page.visible && page.section === "sources" && actions.editingServerKey === ""
        onTriggered: { rows.refresh(); Sources.refresh() }
    }

    Text {
        visible: rows.pending
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        anchors.top: nav.top
        text: I18n.tr("This section is not built yet.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
}
