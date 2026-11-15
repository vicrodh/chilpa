import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Column {
    id: tree
    required property var entries
    required property var controller
    required property string pluginId
    signal entryClicked(var entry)
    function activate(entry) {
        if (entry.route) {
            const saved = SettingsModel.kept("ui.source.collection." + entry.route)
            Navigation.goKey("source", saved || entry.route)
        } else entryClicked(entry)
    }
    Repeater {
        model: tree.entries
        Column {
            id: branch
            required property var modelData
            width: tree.width
            readonly property bool open: tree.controller.expandedSources[tree.pluginId + "/" + modelData.id] === true
            NavItem {
                objectName: "sourceChild"
                width: parent.width - x
                x: Metrics.navItemPadding
                text: branch.modelData.title
                glyph: Icons[branch.modelData.icon] || Icons.folder
                expandable: (branch.modelData.children || []).length > 0
                expanded: branch.open
                active: branch.modelData.route ? Navigation.screen === "source" && NativePages.sameNavigation(Navigation.routeKey, branch.modelData.route, Sources.revision + Library.remoteRev) : tree.controller.sourceView.navigation_id === branch.modelData.id
                onExpansionClicked: {
                    const state = Object.assign({}, tree.controller.expandedSources)
                    state[tree.pluginId + "/" + branch.modelData.id] = !expanded
                    tree.controller.expandedSources = state
                }
                onClicked: {
                    if (expandable && !expanded) {
                        const state = Object.assign({}, tree.controller.expandedSources)
                        state[tree.pluginId + "/" + branch.modelData.id] = true
                        tree.controller.expandedSources = state
                    }
                    tree.activate(branch.modelData)
                }
                onMiddleClicked: clicked()
            }
            Loader {
                id: childLoader
                x: Metrics.navItemPadding
                width: parent.width - x
                active: false
                visible: branch.open
                function loadBranch() {
                    if (!branch.open || (branch.modelData.children || []).length === 0) return
                    if (!item) {
                        setSource(Qt.resolvedUrl("SourceNavigationTree.qml"), {entries:branch.modelData.children,controller:tree.controller,pluginId:tree.pluginId})
                        active = true
                    } else item.entries = branch.modelData.children
                }
                Component.onCompleted: loadBranch()
                Connections { target: branch; function onOpenChanged() { childLoader.loadBranch() } function onModelDataChanged() { childLoader.loadBranch() } }
                Connections { target: childLoader.item; function onEntryClicked(entry) { tree.entryClicked(entry) } }
            }
        }
    }
}
