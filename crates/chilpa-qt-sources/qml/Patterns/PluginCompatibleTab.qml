import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "../Components"

// Plugins made for other platforms that Chilpa can run, from the catalogs
// the listener adds: their filters stay put and the list scrolls below.
Item {
    id: tab
    required property var controller
    readonly property alias searchText: search.text
    property alias catalogView: catalog
    Column {
        id: filters
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Chilpa can run some plugins made for other platforms. Add a plugin catalog you use. Compatibility is not guaranteed; Chilpa is not responsible for these plugins' behavior.", I18n.rev)
            wrapMode: Text.WordWrap
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            Repeater {
                model: [{id: "installed", label: I18n.tr("Installed", I18n.rev)}, {id: "valid", label: I18n.tr("Valid", I18n.rev)}, {id: "uninspected", label: I18n.tr("Not inspected", I18n.rev)}, {id: "incompatible", label: I18n.tr("Not compatible", I18n.rev)}]
                P.PillButton {
                    required property var modelData
                    text: modelData.label + " (" + ((tab.controller.view.counts || {})[modelData.id] || 0) + ")"
                    kind: tab.controller.tab === modelData.id ? "primary" : "secondary"
                    onClicked: { tab.controller.tab = modelData.id; tab.controller.filter() }
                }
            }
        }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            P.TextField {
                id: search
                objectName: "pluginSearch"
                width: Math.max(0, parent.width - categories.width - Metrics.gap8)
                placeholder: I18n.tr("Search plugins", I18n.rev)
                onTextEdited: tab.controller.filter()
                onAccepted: tab.controller.filter()
            }
            P.SelectField {
                id: categories
                width: Math.min(Metrics.selectWidth, parent.width / 2) // derived: reserve at least half for search
                options: [I18n.tr("All categories", I18n.rev)].concat((tab.controller.view.categories || []).map(c => tab.controller.categoryLabel(c)))
                values: [""].concat(tab.controller.view.categories || [])
                value: tab.controller.category
                onPicked: (value) => { tab.controller.category = value; tab.controller.filter() }
            }
        }
    }
    ListView {
        id: catalog
        anchors.top: filters.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        reuseItems: true
        model: tab.controller.catalogRows
        ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }
        delegate: PluginRow {
            required property var modelData
            objectName: "catalogPluginRow"
            width: catalog.width - Metrics.gap12
            plugin: modelData
            installed: tab.controller.tab === "installed"
            enabled: !tab.controller.view.busy && !tab.controller.filterPending
            onMoved: position => tab.controller.command("sidebar_position", {id: modelData.id, position: position})
            onInspected: tab.controller.inspect(modelData)
            onSettings: tab.controller.settings(modelData)
            onOpened: Navigation.goKey("pluginDownloads", modelData.id)
            onSwitched: (on) => tab.controller.command(on ? "review_permissions" : "disable", {id: modelData.id})
        }
        Text {
            anchors.centerIn: parent
            visible: catalog.count === 0 && !tab.controller.view.busy
            text: I18n.tr("No matching plugins", I18n.rev)
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
}
