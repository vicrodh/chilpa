import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
// Qt 6.10 brings a SearchField of its own with QtQuick.Controls; this is the kit's.
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Settings (design/06 Settings): the sections on the left, the chosen one's
// rows on the right. Every row comes from the schema in Rust.
Item {
    id: page

    readonly property string section: Navigation.section !== "" ? Navigation.section : SettingsModel.sections[0]
    // The setting a search went to: its section opens, its row comes into
    // view and lights up for a moment.
    property string focusKey: ""
    function go(setting) {
        search.text = ""
        page.focusKey = setting.key
        Navigation.openSection(setting.section)
        Qt.callLater(page.reveal)
    }
    function reveal() {
        for (let i = 0; i < list.count; i++) {
            if (rows.data(rows.index(i, 0), Metrics.settingsKeyRole) !== page.focusKey)
                continue
            list.positionViewAtIndex(i, ListView.Center)
            flash.target = list.itemAtIndex(i)
            fade.restart()
            return
        }
    }

    SettingsPage {
        id: rows
        section: page.section
    }

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
                text: SettingsModel.sectionLabels[index]
                glyph: Icons[SettingsModel.sectionIcons[index]] || ""
                active: modelData === page.section
                onClicked: Navigation.openSection(modelData)
            }
        }
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
        ScrollBar.vertical: ViewportScrollBar {}

        delegate: SettingsRow {
            width: ListView.view.width - Metrics.gap12
            page: actions
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
        visible: page.section === "plugins" && search.text === ""
        page: rows
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        anchors.bottom: parent.bottom
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.rightMargin: Metrics.contentX
    }

    // What a row does, with the servers' own steps in between: adding one
    // opens its dialog, removing one says first what it takes (ADR-017).
    QtObject {
        id: actions
        function set(key, value) {
            if (key.startsWith("sources.server/") && value === "remove") {
                removing.serverId = Number(key.split("/")[1])
                removing.message = Servers.removal(removing.serverId)
                return removing.open()
            }
            rows.set(key, value)
        }
        function press(key) {
            if (key === "devices.cast")
                return casting.show(actions)
            if (key === "sources.addServer")
                return adding.open()
            rows.press(key)
        }
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
            Servers.remove(serverId)
            rows.refresh()
        }
    }

    // A sync moving, a server coming back: its rows follow while shown.
    Timer {
        interval: Motion.refresh
        repeat: true
        running: page.visible && page.section === "sources"
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
