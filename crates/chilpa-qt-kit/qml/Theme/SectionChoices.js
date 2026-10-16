// I preserve one choice per module when a source consolidates older sections.
function order(available, preferences) {
    const rows = [], seen = new Set()
    function append(section) {
        if (!section || seen.has(section.id)) return
        seen.add(section.id)
        const aliases = section.previous_ids || []
        const exact = preferences.find(choice => choice.id === section.id)
        const previous = preferences.filter(choice => aliases.includes(choice.id))
        const choice = exact || (previous.length ? Object.assign({}, previous[0], {
            enabled: previous.some(row => row.enabled !== false)
        }) : {})
        rows.push(Object.assign({}, section, choice, {id: section.id}))
    }
    for (const choice of preferences)
        append(available.find(section => section.id === choice.id || (section.previous_ids || []).includes(choice.id)))
    for (const section of available) append(section)
    return rows
}
