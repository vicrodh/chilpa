pragma Singleton
import QtQuick

// The window's translator, for tests. It answers in English, the msgid, until
// a test chooses another language: then it reads that language's shipped
// catalog the way chilpa-i18n reads it (msgid, msgstr and plural forms) and
// answers from it, so a surface can be measured in the words it really shows.
// The test runners copy the catalogs next to this file and allow the read.
QtObject {
    id: speaker
    property int rev: 0
    property string language: "en"
    property var singular: ({})
    property var plural: ({})

    function choose(code) {
        const next = code === "" ? "en" : code
        if (next === "en") {
            singular = ({})
            plural = ({})
        } else {
            const catalog = read(next)
            singular = catalog.singular
            plural = catalog.plural
        }
        language = next
        rev++
    }

    function read(code) {
        const request = new XMLHttpRequest()
        request.open("GET", Qt.resolvedUrl("translations/" + code + "/LC_MESSAGES/chilpa.po"), false)
        request.send()
        if (request.responseText === "")
            throw new Error("no catalog for " + code + ": the runner copies them next to I18n.qml and sets QML_XHR_ALLOW_FILE_READ=1")
        return parse(request.responseText)
    }

    function unquote(raw) {
        const inner = raw.trim().replace(/^"/, "").replace(/"$/, "")
        return inner.replace(/\\(.)/g, (whole, c) => c === "n" ? "\n" : c === "t" ? "\t" : c === "r" ? "\r" : c === "\"" ? "\"" : c === "\\" ? "\\" : whole)
    }

    function parse(source) {
        const one = {}, many = {}
        let id = null, idPlural = null, forms = [], slot = ""
        const flush = () => {
            if (id !== null && id !== "" && forms.length > 0) {
                if (idPlural !== null) many[id] = forms
                else one[id] = forms[0]
            }
            id = null; idPlural = null; forms = []; slot = ""
        }
        for (const raw of source.split("\n")) {
            const line = raw.trim()
            if (line === "") { flush(); continue }
            if (line.startsWith("#")) continue
            if (line.startsWith("msgid_plural ")) { idPlural = unquote(line.slice(13)); slot = "plural" }
            else if (line.startsWith("msgid ")) { flush(); id = unquote(line.slice(6)); slot = "id" }
            else if (line.startsWith("msgstr[")) {
                const close = line.indexOf("]")
                const index = parseInt(line.slice(7, close)) || 0
                while (forms.length <= index) forms.push("")
                forms[index] = unquote(line.slice(close + 1))
                slot = "form" + index
            } else if (line.startsWith("msgstr ")) { forms = [unquote(line.slice(7))]; slot = "form0" }
            else if (line.startsWith("\"")) {
                const more = unquote(line)
                if (slot === "id") id += more
                else if (slot === "plural") idPlural += more
                else if (slot.startsWith("form")) forms[parseInt(slot.slice(4))] += more
            }
        }
        flush()
        return { singular: one, plural: many }
    }

    // The plural form a count takes, as chilpa-i18n's PluralRule::for_language.
    function form(n) {
        if (language === "ja") return 0
        if (language === "fr") return n > 1 ? 1 : 0
        if (language === "ru") {
            const ten = Math.abs(n) % 10, hundred = Math.abs(n) % 100
            return ten === 1 && hundred !== 11 ? 0 : ten >= 2 && ten <= 4 && (hundred < 12 || hundred > 14) ? 1 : 2
        }
        return n === 1 ? 0 : 1
    }

    // Each {} takes the next argument and {1}, {2}... that argument; a hole
    // without one stays, as chilpa-i18n's fill.
    function fill(text, args) {
        let next = 0
        return text.replace(/\{(\d*)\}/g, (hole, digits) => {
            const at = digits === "" ? next++ : parseInt(digits) - 1
            return at >= 0 && at < args.length ? String(args[at]) : hole
        })
    }

    function tr(value, revision) {
        const found = singular[value]
        return found ? found : value
    }
    function trN(one, many, count, revision) {
        const forms = plural[one]
        const found = forms ? forms[form(count)] : ""
        return found ? found : count === 1 ? one : many
    }
    function trArgs(value, args, revision) { return fill(tr(value, revision), args) }
    function trCount(one, many, count, revision) { return fill(trN(one, many, count, revision), [String(count)]) }
}
