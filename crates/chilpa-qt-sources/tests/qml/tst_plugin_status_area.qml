import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../../qml/Patterns"

TestCase {
    id: test
    name: "PluginStatusArea"
    when: windowShown
    visible: true
    width: 900
    height: 850
    Modal {
        id: dialog
        preferredWidth: 500
        PluginSettingsForm {
            id: form
            width: parent.width
            pluginId: "fixture.id"
            document: ({})
        }
    }
    function document(waiting, message, step) {
        return {
            revision:1,
            wizard:{step:step || 3,waiting:waiting,description:message,title:"Playback",steps:[
                {label:"Prepare"},{label:"Library"},{label:"Playback"},{label:"Ready"}]},
            status:{phase:waiting ? "running" : "pending",label:"Playback connection"},
            form:{fields:waiting || step === 4 ? [] : [
                {key:"email",label:"Account",control:{type:"text"}},
                {key:"password",label:"Password",control:{type:"secret"}}],
                actions:[{key:waiting ? "cancel" : "connect",label:waiting ? "Cancel" : "Connect",fields:[]}]},
            values:{},notices:[],content:[]
        }
    }
    function init() {
        form.hostStatus = ""
        form.hostBusy = false
        form.document = document(false,"Connect this account for playback.",3)
        dialog.open()
        tryCompare(dialog,"opened",true)
        waitForRendering(test)
    }
    function cleanup() { dialog.close(); tryCompare(dialog,"visible",false) }
    function update(doc) { form.document = doc; waitForRendering(test) }
    function test_validation_messages_and_progress_keep_the_modal_size() {
        const height = form.implicitHeight, modalHeight = dialog.implicitHeight
        const status = findChild(form,"pluginValidationArea")
        const statusHeight = status.height
        form.values = {password:"fixture-input"}
        const pending = document(true,"The account is being checked.",3)
        pending.notices = ["A longer temporary response. ".repeat(50)]
        pending.progress = {done:5,total:10}
        update(pending)
        compare(form.implicitHeight,height)
        compare(dialog.implicitHeight,modalHeight)
        compare(status.height,statusHeight)
        const viewport = findChild(status,"statusViewport")
        verify(viewport.contentHeight > viewport.height)
        const password = findChild(form,"pluginField_password")
        verify(password)
        verify(!password.enabled)
        compare(password.text,"")
        compare(Object.keys(form.values).length,0)
        update(document(true,"Still checking.",3))
        compare(form.implicitHeight,height)
        form.hostStatus = "Loading…"
        form.hostBusy = true
        waitForRendering(test)
        compare(form.implicitHeight,height)
        form.hostStatus = ""
        form.hostBusy = false
    }
    function test_a_new_step_releases_the_previous_fields_and_resizes_to_its_content() {
        const previous = form.implicitHeight
        update(document(true,"Checking.",3))
        compare(form.shownFields.length,2)
        update(document(false,"Connected.",4))
        compare(form.shownFields.length,0)
        verify(form.implicitHeight < previous)
        compare(findChild(form,"pluginField_password"),null)
    }
    function test_closing_releases_the_retained_field_descriptors() {
        update(document(true,"Checking.",3))
        compare(form.shownFields.length,2)
        dialog.close()
        tryCompare(dialog,"visible",false)
        compare(form.shownFields.length,0)
        compare(Object.keys(form.values).length,0)
    }
}
