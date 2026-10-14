function set(actions, rows, removing, servers, language, key, value) {
    if (key === "appearance.language") { language.choose(value); rows.refresh(); return }
    if (key.startsWith("sources.server/") && value === "edit") { servers.begin("edit/" + key.split("/")[1]); actions.editingServerKey = key; return }
    if (key.startsWith("sources.server/") && value === "close-edit") { servers.cancel(); actions.editingServerKey = ""; rows.refresh(); return }
    if (key.startsWith("sources.server/") && value === "remove") { removing.serverId = Number(key.split("/")[1]); removing.message = servers.removal(removing.serverId); return removing.open() }
    rows.set(key, value)
}

function press(actions, rows, manager, casting, adding, diagnosis, inventory, navigation, servers, qt, key) {
    if (key === "library.analysis.inventory") return inventory.show()
    if (key === "library.mood") {
        navigation.openSection("plugins")
        qt.callLater(() => {
            manager.read()
            const plugin = manager.installedRows.find(row => row.id === "app.chilpa.mood")
            if (plugin) manager.settings(plugin)
        })
        return
    }
    if (key === "devices.cast") return casting.show(actions)
    if (key === "sources.addServer") { servers.cancel(); actions.editingServerKey = ""; return adding.open() }
    if (key === "audio.wizard" && rows.diagnosis_requires_stop()) return diagnosis.request()
    rows.press(key)
}
