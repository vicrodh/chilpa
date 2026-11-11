import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Settings > Plugins and Settings > Sources say their headings, buttons
// and column titles in each language without running into each other, at
// the settings page's width and a narrow one, also a tenth wider than Inter.
TestCase {
    id: test
    name: "SourcesTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 960
    height: 700

    QtObject {
        id: backend
        property var state: ({ busy: false, repositories: [{ id: "repo", name: "Community repository", url: "https://example.org/plugins.json" }],
            catalog: [{ id: "entry", name: "Guest source with a long name", light: "yellow", repository_id: "repo", description: "Streams from a service the person signs in to" }],
            installed: [{ id: "fixture", name: "Source", native: true, enabled: true, version: "0.4.11" }] })
        function pluginsJson() { return JSON.stringify(state) }
        function pluginCommand(action, input) {}
    }
    Component { id: managerFactory; PluginManager { height: 600; page: backend } }
    Component { id: serversFactory; MusicServers { height: 400 } }

    readonly property var servers: [{ key: "sources.server/0", title: "Living room server", description: "Connection details", action: "Connected",
        value: JSON.stringify({ icon: "navidrome", product: "Navidrome 0.64.2", tracks: 123456 }),
        options: ["Edit…", "Disable", "Sync now", "Remove…"], optionValues: ["edit", "disable", "sync", "remove"] }]

    function cleanup() { I18n.choose("en") }

    function test_plugins_data() { return wide(TextAudit.cases()) }
    function test_plugins(data) {
        I18n.choose(data.language)
        const manager = createTemporaryObject(managerFactory, test, { width: data.width })
        manager.read()
        for (const section of ["native", "compatible"]) {
            manager.section = section
            const found = TextAudit.check(test, manager, data.scale)
            compare(found.length, 0, section + ":\n" + found.join("\n"))
        }
    }

    function test_servers_data() { return wide(TextAudit.cases()) }
    function test_servers(data) {
        I18n.choose(data.language)
        SettingsModel.preferences = ({})
        const table = createTemporaryObject(serversFactory, test, { width: data.width, model: test.servers })
        const found = TextAudit.check(test, table, data.scale)
        compare(found.length, 0, found.join("\n"))
    }

    // Each case at the settings page's width in a usual window and in a
    // narrow one.
    function wide(cases) {
        const out = []
        for (const row of cases)
            for (const width of [520, 600, 760])
                out.push({ tag: row.tag + " " + width, language: row.language, scale: row.scale, width: width })
        return out
    }
}
