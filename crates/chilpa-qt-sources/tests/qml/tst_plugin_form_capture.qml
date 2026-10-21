import QtQuick
import QtTest
import "../../qml/Patterns"

TestCase {
    id: test
    name: "PluginFormCapture"
    when: windowShown
    visible: true
    width: 600
    height: 400

    PluginSettingsForm {
        id: form
        width: 400
        captures: true
        document: ({
            links: [
                { label: "Sign in", url: "https://login.example.test/a", primary: true,
                  capture: { until: "https://example.test/return", action: "finish", field: "redirect_url" } },
                { label: "Help", url: "https://example.test/help" }
            ],
            form: { fields: [], actions: [] }
        })
    }

    SignalSpy { id: captured; target: form; signalName: "captured" }
    SignalSpy { id: linked; target: form; signalName: "linked" }

    function button(label) {
        let found = null
        const walk = (item) => {
            if (found) return
            if (item.text === label && item.clicked !== undefined) found = item
            for (const child of item.children || []) walk(child)
        }
        walk(form)
        return found
    }

    function test_a_link_with_a_return_address_asks_for_the_sign_in_window() {
        tryVerify(() => button("Sign in") !== null)
        linked.clear()
        captured.clear()
        mouseClick(button("Sign in"))
        compare(captured.count, 1)
        compare(linked.count, 0)
        compare(captured.signalArguments[0][0].capture.until, "https://example.test/return")
        compare(captured.signalArguments[0][0].url, "https://login.example.test/a")
    }

    function test_without_a_sign_in_window_the_link_opens_as_before() {
        tryVerify(() => button("Sign in") !== null)
        linked.clear()
        captured.clear()
        form.captures = false
        mouseClick(button("Sign in"))
        form.captures = true
        compare(captured.count, 0)
        compare(linked.count, 1)
        compare(linked.signalArguments[0][0], "https://login.example.test/a")
    }

    function test_a_plain_link_still_opens_as_a_link() {
        tryVerify(() => button("Help") !== null)
        linked.clear()
        captured.clear()
        mouseClick(button("Help"))
        compare(linked.count, 1)
        compare(captured.count, 0)
        compare(linked.signalArguments[0][0], "https://example.test/help")
    }
}
