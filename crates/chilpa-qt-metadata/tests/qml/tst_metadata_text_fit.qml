import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.metadata
import "../../qml/Patterns"

// The editor's head says where a server's record saves -- the server, the
// sidecar on this device, or why the server cannot take it -- in each
// language without running into its buttons or out of its box, from a
// narrow window to a wide one, also a tenth wider than Inter.
TestCase {
    id: test
    name: "MetadataTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1000
    height: 400

    Component { id: factory; MetadataHeader {} }

    function cleanup() { I18n.choose("en") }

    function cases() {
        const out = []
        for (const row of TextAudit.cases())
            for (const width of [520, 860])
                for (const mode of ["server", "sidecar", ""])
                    out.push({ tag: row.tag + " " + width + " " + (mode || "undecided"), language: row.language, scale: row.scale, width: width, mode: mode })
        return out
    }

    function test_where_a_server_s_record_saves_data() { return cases() }
    function test_where_a_server_s_record_saves(data) {
        I18n.choose(data.language)
        // The facade says why in the listener's language; the catalog the
        // window reads says the same.
        const refused = data.mode === "" ? I18n.tr("only an administrator of this server can change its metadata", I18n.rev) : ""
        const doc = { state: "ready", dirty: true, error: "", notice: "", album: [{ name: "title", value: "Es ist Abend und wir sitzen bei mir" }], tracks: [{}, {}, {}],
                      save: { mode: data.mode, hosted: true, files_allowed: false, files_refused: "", xml_allowed: false,
                              server: { name: "Living room server with a long name", allowed: data.mode !== "", refused: refused } },
                      layers: { present: [] } }
        const head = createTemporaryObject(factory, test, { width: data.width, doc: doc })
        const found = TextAudit.check(test, head, data.scale)
        compare(found.length, 0, found.join("\n"))
    }
}
