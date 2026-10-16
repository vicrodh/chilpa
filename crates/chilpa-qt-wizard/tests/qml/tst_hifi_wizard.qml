import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.wizard
import "../../qml/Components"

// The wizard opens with its document and starts over on every open; each
// step's Next waits for what QBZ's waits for; entering the DACs finds them
// and entering the review writes the files; the read-back shows its check.
TestCase {
    id: test
    name: "HifiWizard"
    when: windowShown
    visible: true
    width: 900
    height: 800

    HifiWizardModal { id: modal }

    function show(doc) {
        HifiWizard.doc = JSON.stringify(Object.assign({ open: true, openSeq: 1 }, doc))
        tryCompare(modal, "visible", true)
        waitForRendering(modal.contentItem)
    }
    function press(name) {
        mouseClick(child(name))
        waitForRendering(modal.contentItem)
    }
    function child(name) { return findChild(modal.contentItem, name) }
    // A step that just showed is placed on the next polish.
    function goTo(step) {
        modal.step = step
        waitForRendering(modal.contentItem)
    }

    function cleanup() {
        HifiWizard.doc = "{}"
        HifiWizard.asked = []
        tryCompare(modal, "visible", false)
    }

    function test_the_terms_open_the_start_and_entering_the_dacs_finds_them() {
        show({})
        compare(modal.step, 0)
        verify(!child("wizardPrimary").enabled, "the terms first")
        press("wizardAccept")
        verify(child("wizardPrimary").enabled)
        press("wizardPrimary")
        compare(modal.step, 1)
        press("wizardPrimary")
        compare(modal.step, 2)
        compare(HifiWizard.asked, ["runDetect:"])
        verify(!child("wizardPrimary").enabled, "no DAC chosen yet")
    }

    function test_a_chosen_dac_writes_the_files_and_the_review_waits_for_its_boxes() {
        show({ candidates: [{ id: "alsa_output.usb-x", description: "DAC", checked: false }], hasEnumeration: true })
        goTo(2)
        mouseClick(child("wizardDac0"))
        compare(HifiWizard.asked, ["toggleDac:0"])
        HifiWizard.doc = JSON.stringify({ open: true, openSeq: 1, anyDacSelected: true, candidates: [{ id: "alsa_output.usb-x", checked: true }] })
        verify(child("wizardPrimary").enabled)
        press("wizardPrimary")
        compare(modal.step, 3)
        verify(HifiWizard.asked.indexOf("genConfigs:") >= 0)
        verify(!child("wizardPrimary").enabled)
        for (const box of ["wizardBackup", "wizardConfigured", "wizardRestarted"])
            press(box)
        verify(child("wizardPrimary").enabled, "the three boxes")
    }

    function test_the_manual_entry_hides_until_asked_unless_nothing_was_found() {
        show({ hasEnumeration: true })
        goTo(2)
        verify(!child("wizardManual").visible)
        mouseClick(child("wizardManualToggle"))
        waitForRendering(modal.contentItem)
        verify(child("wizardManual").visible)
        HifiWizard.doc = JSON.stringify({ open: true, openSeq: 1, hasEnumeration: false })
        modal.showManual = false
        verify(child("wizardManual").visible, "open when nothing was found")
    }

    function test_the_read_back_checks_a_matching_rate() {
        show({ testPlaying: true, testTracks: ["16-bit / 44.1 kHz — Song"], testRequestedLabel: "Chilpa requesting 96 kHz · 24-bit", testNegotiatedLabel: "DAC: 96 kHz · S32_LE · 2 ch", testRateMatched: true })
        goTo(4)
        verify(child("wizardReadback").visible)
        verify(child("wizardMatched").visible)
        mouseClick(child("wizardTrack0"))
        verify(HifiWizard.asked.indexOf("testPlayIndex:0") >= 0, "a row jumps to its track while the test plays")
        mouseClick(child("wizardPlay"))
        verify(HifiWizard.asked.indexOf("stopTest:") >= 0)
    }

    function test_every_open_starts_over_and_closing_only_hides() {
        show({})
        modal.step = 3
        modal.welcomeConfirmed = true
        HifiWizard.doc = JSON.stringify({ open: true, openSeq: 2 })
        compare(modal.step, 0)
        verify(!modal.welcomeConfirmed)
        mouseClick(child("wizardClose"))
        tryCompare(modal, "visible", false)
        compare(HifiWizard.asked, ["close:"])
    }
}
