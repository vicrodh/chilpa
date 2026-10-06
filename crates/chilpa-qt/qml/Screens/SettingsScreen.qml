import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Patterns"
import "../Components"
import "../Patterns"

// Settings (design/06 Settings): the sections on the left, the chosen one's
// rows on the right. Every row comes from the schema in Rust.
Item {
    id: page

    readonly property string section: Navigation.section !== "" ? Navigation.section : SettingsModel.sections[0]

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

    Column {
        id: nav
        x: Metrics.contentX
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
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

    ListView {
        visible: page.section !== "plugins"
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

    PluginManager {
        visible: page.section === "plugins"
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
