pragma Singleton
import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// How the Refine panel names a condition: its field, its glyph, its
// operator and its values, as the design writes them (`valStr`, `rangeStr`
// and `ratingStr` in the handoff's source). A row is the facade's JSON:
// {field, kind, negated, values, from, to, min, max, n, unit} or a group.
QtObject {
    // The fields in the order the panel offers them.
    readonly property var order: ["genre", "year", "artist", "composer", "label", "rating", "added", "album", "title", "plays", "type", "quality"]

    function field(name) {
        switch (name) {
        case "genre": return I18n.tr("Genre", I18n.rev)
        case "year": return I18n.tr("Year", I18n.rev)
        case "artist": return I18n.tr("Artist", I18n.rev)
        case "composer": return I18n.tr("Composer", I18n.rev)
        case "label": return I18n.tr("Label", I18n.rev)
        case "rating": return I18n.tr("Rating", I18n.rev)
        case "added": return I18n.tr("Added", I18n.rev)
        case "album": return I18n.tr("Album", I18n.rev)
        case "title": return I18n.tr("Title", I18n.rev)
        case "plays": return I18n.tr("Plays", I18n.rev)
        case "type": return I18n.tr("Type", I18n.rev)
        case "quality": return I18n.tr("Quality", I18n.rev)
        default: return I18n.tr("Words", I18n.rev)
        }
    }
    function glyph(name) {
        switch (name) {
        case "genre": return Icons.tag
        case "year": return Icons.calendar
        case "artist": return Icons.user
        case "composer": return Icons.feather
        case "label": return Icons.building
        case "rating": return Icons.star
        case "added": return Icons.clockPlus
        case "album": return Icons.disc
        case "title": return Icons.music
        case "plays": return Icons.playerPlay
        case "type": return Icons.disc
        case "quality": return Icons.waveSine
        default: return Icons.letterCase
        }
    }
    function unit(n, unit) {
        switch (unit) {
        case "d": return I18n.trCount("{} day", "{} days", n, I18n.rev)
        case "w": return I18n.trCount("{} week", "{} weeks", n, I18n.rev)
        case "m": return I18n.trCount("{} month", "{} months", n, I18n.rev)
        default: return I18n.trCount("{} year", "{} years", n, I18n.rev)
        }
    }
    function range(row) {
        if (row.from && row.to)
            return row.from === row.to ? String(row.from) : row.from + " – " + row.to
        return row.from ? "≥ " + row.from : "≤ " + row.to
    }
    function stars(row) {
        if (row.min === 0 && row.max === 0) return I18n.tr("unrated", I18n.rev)
        if (row.min === row.max || row.max === 5) return row.min + "★"
        return row.min + "★ – " + row.max + "★"
    }
    // The value as one line, for a summary chip.
    function summary(row) {
        if (row.group) return I18n.trCount("{} condition", "{} conditions", row.rows.length, I18n.rev)
        switch (row.kind) {
        case "none": return I18n.tr("has none", I18n.rev)
        case "range": return range(row)
        case "rating": return row.max === 5 && row.min > 0 && row.min < 5 ? "≥ " + stars(row) : stars(row)
        case "added": return I18n.tr("in the last", I18n.rev) + " " + unit(row.n, row.unit)
        case "words": return row.values.map(v => "“" + v + "”").join(", ")
        default: return row.values.join(", ")
        }
    }
    // The key a summary chip starts with.
    function key(row) {
        if (row.group) return row.mode === "any" ? I18n.tr("Any of these", I18n.rev) : I18n.tr("All of these", I18n.rev)
        return field(row.field)
    }
}
