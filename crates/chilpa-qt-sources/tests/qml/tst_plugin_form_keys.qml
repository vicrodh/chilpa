import QtQuick
import QtQuick.Window
import QtTest
import "../../qml/Patterns"

TestCase {
    id: test
    name: "PluginFormKeys"
    when: windowShown
    visible: true
    width: 600
    height: 700

    PluginSettingsForm {
        id: form
        width: 400
        document: ({
            form: {
                fields: [
                    { key: "user", label: "User", control: { type: "text" } },
                    { key: "password", label: "Password", control: { type: "secret" } },
                    { key: "remember", label: "Remember", control: { type: "toggle" } },
                    { key: "quality", label: "Quality", control: { type: "select", choices: [{ label: "High", value: "high" }, { label: "Low", value: "low" }] } }
                ],
                actions: [{ key: "save", label: "Save", fields: ["user", "password", "remember", "quality"] }]
            }
        })
    }

    function kind() {
        const item = test.Window.window.activeFocusItem
        if (!item) return ""
        if (item instanceof TextInput) return item.echoMode === TextInput.Password ? "secret" : "text"
        return item.objectName || String(item)
    }

    function test_the_fields_of_a_plugin_take_tab_in_their_order() {
        tryVerify(() => findChild(form, "pluginToggle") !== null)
        let input = null
        const walk = (item) => { if (!input && item instanceof TextInput) input = item; for (const c of item.children || []) walk(c) }
        walk(form)
        input.forceActiveFocus()
        const order = [kind()]
        for (let i = 0; i < 4; i++) {
            keyClick(Qt.Key_Tab)
            order.push(kind())
        }
        compare(order.slice(0, 4), ["text", "secret", "pluginToggle", "pluginSelect"])
        verify(order[4] !== "pluginSelect" && order[4] !== "", "the action comes after the fields: " + order[4])
    }
}
