import QtQuick
import QtTest
import "../../qml/Patterns"

TestCase {
    name: "PluginFormConfirmations"
    when: windowShown
    visible: true
    width: 700
    height: 700
    PluginSettingsForm { id: form; width: 400; pluginName: "Fixture"; document: ({}) }
    SignalSpy { id: submitted; target: form; signalName: "submitted" }
    function init() {
        submitted.clear()
        form.pluginId = "fixture.id"
        form.document = {revision:1,form:{fields:[],actions:[{key:"disconnect",label:"Disconnect",fields:["password"],destructive:"account"}]}}
        form.values = {password:"fixture-input",unrelated:"excluded"}
    }
    function question() { return findChild(form,"pluginActionConfirmation") }
    function ask() {
        findChild(form,"pluginAction_disconnect").clicked()
        tryCompare(question(),"opened",true)
        tryCompare(question(),"targetValid",true)
    }
    function first() {
        findChild(question().contentItem,"confirmationAccept").clicked()
        compare(submitted.count,0)
        findChild(question().contentItem,"destructiveAcknowledgement").checked = true
    }
    function cleanup() { question().close() }
    function test_a_declared_destructive_action_requires_both_confirmations() {
        ask()
        compare(submitted.count,0)
        first()
        findChild(question().contentItem,"confirmationAccept").clicked()
        compare(submitted.count,1)
        compare(submitted.signalArguments[0][0],"disconnect")
        compare(Object.keys(submitted.signalArguments[0][1]).join(","),"password")
        compare(Object.keys(question().payload).length,0)
        compare(Object.keys(form.values).length,0)
        compare(submitted.signalArguments[0][2].plugin,"fixture.id")
        compare(submitted.signalArguments[0][2].revision,1)
    }
    function test_replacing_settings_requires_one_confirmation() {
        form.document = {revision:1,form:{fields:[],actions:[{key:"disconnect",label:"Reset",fields:[],destructive:"settings"}]}}
        ask()
        findChild(question().contentItem,"confirmationAccept").clicked()
        compare(submitted.count,1)
    }
    function test_cancel_discards_the_captured_payload_without_submitting() {
        ask()
        first()
        question().close()
        tryCompare(question(),"visible",false)
        compare(submitted.count,0)
        compare(Object.keys(question().payload).length,0)
    }
    function test_a_new_form_revision_invalidates_the_pending_action() {
        ask()
        first()
        form.document = {revision:2,form:{fields:[],actions:[]}}
        tryCompare(question(),"targetValid",false)
        findChild(question().contentItem,"confirmationAccept").clicked()
        compare(submitted.count,0)
    }
    function test_legacy_nondestructive_actions_keep_their_submission_flow() {
        form.document = {revision:2,form:{fields:[],actions:[{key:"save",label:"Save",fields:[]}]}}
        findChild(form,"pluginAction_save").clicked()
        compare(submitted.count,1)
        verify(!question().visible)
    }
    function test_a_different_plugin_with_the_same_form_cannot_receive_the_action() {
        ask()
        first()
        form.pluginId = "another.id"
        tryCompare(question(),"targetValid",false)
        findChild(question().contentItem,"confirmationAccept").clicked()
        compare(submitted.count,0)
    }
}
