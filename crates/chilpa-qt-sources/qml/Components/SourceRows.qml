import QtQml
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources

// The sources a Library screen can show, and their switches: what the
// library lists, then the sources that can join it and do not show now. A
// source's own switch is on when either of its libraries is in and it is
// not kept out. Read only while `active`.
QtObject {
    id: source

    property bool active: false
    readonly property int rev: Library.remoteRev + Library.rev + Sources.revision
    readonly property var rows: active ? read(rev, Library.sourceKeys, Library.excludedSources, I18n.rev) : []
    // The sources on, by their first word, or "All music" with every one on.
    readonly property string label: {
        const on = rows.filter(row => row.on)
        if (on.length === rows.length) return I18n.tr("All music", I18n.rev)
        if (on.length === 0) return I18n.tr("No sources", I18n.rev)
        return on.map(row => row.key === "local:default" ? I18n.tr("Local", I18n.rev) : row.label.split(" ")[0]).join(" · ")
    }

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
}
