import QtQuick
import QtTest
import "../../qml/Patterns"

TestCase {
    id: test
    name: "PluginSetupWizard"
    when: windowShown
    visible: true
    width: 640
    height: 700

    PluginSettingsForm {
        id: form
        width: 480
        document: ({
            wizard: {step: 3, total: 4, title: "Connect playback", description: "Finish account verification here.",
                steps: [{label: "Prepare", state: "complete"}, {label: "Library", state: "complete"},
                    {label: "Playback", state: "current"}, {label: "Ready", state: "upcoming"}]},
            form: {fields: [{key: "reply", label: "Verification code", control: {type: "text"}}],
                actions: [{key: "verify", label: "Verify", fields: ["reply"]}]}, values: {}, links: [], notices: []
        })
    }
    SignalSpy { id: submitted; target: form; signalName: "submitted" }

    function test_steps_and_inline_verification_remain_inside_the_form() {
        tryVerify(() => findChild(form, "setupStep_3") !== null)
        compare(findChild(form, "setupWizardTitle").text, "Connect playback")
        const input = findChild(form, "pluginField_reply")
        verify(input !== null)
        input.text = "123456"
        input.textEdited()
        let button = null
        const walk = item => {
            if (item.text === "Verify" && item.clicked !== undefined) button = item
            for (const child of item.children || []) walk(child)
        }
        walk(form)
        verify(button !== null)
        compare(button.kind, "primary")
        mouseClick(button)
        compare(submitted.count, 1)
        compare(submitted.signalArguments[0][0], "verify")
        compare(submitted.signalArguments[0][1].reply, "123456")
        compare(Object.keys(form.values).length, 0)
    }

    function test_stepper_fits_a_narrow_panel_without_hiding_numbers() {
        form.width = 260
        waitForRendering(form)
        for (let index = 0; index < 4; index++) {
            const marker = findChild(form, "setupStep_" + index)
            verify(marker !== null)
            const position = marker.mapToItem(form, 0, 0)
            verify(position.x >= 0 && position.x + marker.width <= form.width,
                "step " + index + " x=" + position.x + " width=" + marker.width + " panel=" + form.width)
        }
        form.width = 480
    }
}
