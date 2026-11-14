.pragma library

function initial(settings) {
    const saved = settings.kept("ui.library.columns")
    if (saved) { try { const columns = JSON.parse(saved); if (Array.isArray(columns)) return columns } catch (_) {} }
    return (settings.kept("ui.library.panes") || "year,genre").split(",").filter(c => c !== "" && c !== "-").concat(["artist", "album"])
}

function toggle(columns, column, settings, library) {
    const on = columns.indexOf(column) < 0
    const chosen = on ? columns.concat([column]) : columns.filter(c => c !== column)
    settings.keep("ui.library.columns", JSON.stringify(chosen))
    // A column turned off does not narrow what it no longer shows.
    if (!on) {
        library.searchColumn(column, "")
        library.clearColumn(column)
    }
    return chosen
}

function reorder(columns, order, settings) {
    const chosen = order.filter((column, index) => columns.indexOf(column) >= 0 && order.indexOf(column) === index)
        .concat(columns.filter(column => order.indexOf(column) < 0))
    settings.keep("ui.library.columns", JSON.stringify(chosen))
    return chosen
}
