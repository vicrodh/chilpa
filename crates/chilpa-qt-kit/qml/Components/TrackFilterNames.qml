import QtQuick
import app.chilpa.kit
import "../Theme/AudioLabels.js" as AudioLabels

// Display names are shared by the checklist and its active-filter trail.
QtObject {
    property var sourceNames: ({})
    readonly property var fields: ({title: I18n.tr("Title", I18n.rev), artist: I18n.tr("Artist", I18n.rev),
        album: I18n.tr("Album", I18n.rev), source: I18n.tr("Source", I18n.rev), rating: I18n.tr("Rating", I18n.rev),
        quality: I18n.tr("Quality", I18n.rev), format: I18n.tr("Format", I18n.rev), favorite: I18n.tr("Favorite", I18n.rev),
        bits: I18n.tr("Bit depth", I18n.rev), rate: I18n.tr("Sample rate", I18n.rev), search: I18n.tr("Search", I18n.rev)})
    function value(field, key, label) {
        if ((field === "bits" || field === "rate") && key) return AudioLabels.display(field, key)
        if (field === "favorite") return String(key) === "true" ? I18n.tr("Yes", I18n.rev) : I18n.tr("No", I18n.rev)
        if (field === "rating") return key === "0" ? I18n.tr("Unrated", I18n.rev) : key + " / 5"
        if (field === "quality") return ({lossy: I18n.tr("Lossy", I18n.rev), lossless: I18n.tr("Lossless", I18n.rev), hires: I18n.tr("Hi-Res", I18n.rev), dsd: "DSD"})[key] || I18n.tr("Unknown", I18n.rev)
        if (field === "format") return key ? key.toUpperCase() : I18n.tr("Unknown", I18n.rev)
        if (field === "source") return label || sourceNames[key] || I18n.tr("Source", I18n.rev)
        return label || key || I18n.tr("Unknown", I18n.rev)
    }
    function selectedSources(current, selected) {
        const names = ({})
        for (const key of selected.slice(0, 256)) if (current[key]) names[key] = current[key]
        return names
    }
    function rememberSources(current, selected, items) {
        const next = Object.assign({}, current)
        for (const item of items) if (selected.indexOf(item.key) >= 0 && (Object.keys(next).length < 256 || next[item.key])) next[item.key] = item.label
        return next
    }
    function summary(field, selected) {
        const values = Array.isArray(selected) ? selected : [selected]
        return values.slice(0, 2).map(key => value(field, String(key), "")).join(", ")
            + (values.length > 2 ? " (+" + (values.length - 2) + ")" : "")
    }
}
