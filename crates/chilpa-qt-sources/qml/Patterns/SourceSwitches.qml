import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One switch per source, the same in Library, Albums and Tracks: a source's
// switch on puts its saved releases and tracks in all three and shows them;
// off, it is in none. The library's own files and a server's are shown or
// kept out. A source turned off stays listed, so it can be turned on again.
Column {
    id: switches

    property bool active: false
    readonly property int rev: Library.remoteRev + Library.rev + Sources.revision
    readonly property var rows: active ? read(rev, Library.sourceKeys, Library.excludedSources) : []

    // Each source's row: what the library lists, then the sources that can
    // join it and do not show now. A source's own switch is on when either
    // of its libraries is in and it is not kept out.
    function read() {
        const plugins = ({})
        const take = (listed, action) => listed.forEach(entry => {
            const held = plugins[entry.id] || (plugins[entry.id] = { id: entry.id, name: entry.name, icon: entry.icon, actions: [], on: false })
            held.actions.push(action)
            held.on = held.on || entry.enabled === true
        })
        take(JSON.parse(Sources.albumLibraries(rev) || "[]"), "album_library")
        take(JSON.parse(Sources.trackLibraries(rev) || "[]"), "track_library")
        const rows = Library.sourceKeys.map((key, at) => ({ key: key, icon: Library.sourceIcons[at] || "", count: Number(Library.sourceCounts[at]),
            label: key === "local:default" ? I18n.tr("Local library", I18n.rev) : (Library.sourceLabels[at] || ""),
            plugin: plugins[key.replace(/^plugin:/, "")] || null }))
        for (const id in plugins)
            if (!rows.some(row => row.key === "plugin:" + id))
                rows.push({ key: "plugin:" + id, icon: plugins[id].icon, count: -1, label: plugins[id].name, plugin: plugins[id] })
        return rows.map(row => Object.assign(row, { on: (!row.plugin || row.plugin.on) && Library.excludedSources.indexOf(row.key) < 0 }))
    }
    function flip(row, on) {
        if (row.plugin)
            row.plugin.actions.forEach(action => Sources.command(action, row.plugin.id, on ? "true" : "false"))
        // A source switched either way is never left kept out.
        if (!row.plugin || Library.excludedSources.indexOf(row.key) >= 0)
            Library.toggleFilter("source", row.key)
    }

    spacing: Metrics.gap8

    Text {
        text: I18n.tr("Sources", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    Repeater {
        model: switches.rows

        Item {
            required property var modelData
            width: switches.width
            height: Metrics.sourceFilterRow

            SourceIcon {
                id: sourceIcon
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.iconNav
                height: Metrics.iconNav
                sourceInfo: ({ icon: modelData.icon, local: modelData.key === "local:default" })
            }
            Column {
                anchors.left: sourceIcon.right
                anchors.leftMargin: Metrics.gap10
                anchors.right: toggle.left
                anchors.rightMargin: Metrics.gap10
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    text: modelData.label
                    elide: Text.ElideRight
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Text {
                    visible: modelData.count >= 0
                    text: I18n.trCount("{} track", "{} tracks", modelData.count, I18n.rev)
                    font.pointSize: Type.micro
                    color: Theme.color.textTertiary
                }
            }
            Toggle {
                id: toggle
                objectName: "sourceSwitch-" + modelData.key
                compact: true
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: modelData.on
                onToggled: (on) => switches.flip(modelData, on)
            }
        }
    }
}
