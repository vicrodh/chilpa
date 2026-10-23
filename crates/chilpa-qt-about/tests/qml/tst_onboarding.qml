import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.about
import "../../qml/Components"

// The deck asks what this start shows until the facade answers; the Welcome
// walks its cards and closes with its last button; a card's button closes it
// and names where to go; What's new ends in its release notes; every way out
// marks the version seen.
TestCase {
    id: test
    name: "Onboarding"
    when: windowShown
    visible: true
    width: 1200
    height: 820

    OnboardingModal { id: modal }
    SignalSpy { id: chosen; target: modal; signalName: "actionChosen" }

    readonly property var welcomeDeck: [
        { image: "welcome.png", title: "A player for your own music", body: "One", actionLabel: "", action: "" },
        { image: "your-music.png", title: "Your music", body: "Two", actionLabel: "Add a folder", action: "libraryFolders" },
        { image: "plugins.png", title: "Servers and plugins", body: "Three", actionLabel: "Open plugins", action: "plugins" }
    ]

    function init() {
        Onboarding.asked = []
        chosen.clear()
    }
    function cleanup() {
        Onboarding.doc = JSON.stringify({ started: true, open: false })
        tryCompare(modal, "visible", false)
    }
    function show(doc) {
        Onboarding.doc = JSON.stringify(Object.assign({ started: true, open: true }, doc))
        tryCompare(modal, "opened", true)
        waitForRendering(modal.contentItem)
    }
    function child(name) { return findChild(modal.contentItem, name) }
    function press(name) {
        mouseClick(child(name))
        waitForRendering(modal.contentItem)
    }

    function test_the_start_is_asked_until_the_facade_answers() {
        Onboarding.doc = JSON.stringify({ started: false, open: false })
        tryVerify(() => Onboarding.asked.indexOf("start") >= 0, 3000, "the window asks what this start shows")
        Onboarding.doc = JSON.stringify({ started: true, open: false })
        const asked = Onboarding.asked.length
        wait(1500)
        compare(Onboarding.asked.length, asked, "and stops once it has the answer")
    }

    function test_the_welcome_walks_its_cards_and_closes_with_its_last_button() {
        show({ kind: "welcome", version: "0.1.0", deck: welcomeDeck })
        compare(child("deckHeading").text, "Welcome to Chilpa")
        compare(child("cardTitle").text, "A player for your own music")
        verify(!child("deckBack").visible, "nothing to go back to on the first card")
        verify(!child("cardAction").visible, "the first card leads nowhere")
        compare(child("deckNext").text, "Next")
        press("deckNext")
        compare(child("cardTitle").text, "Your music")
        verify(child("deckBack").visible)
        keyClick(Qt.Key_Right)
        compare(modal.cardIndex, 2, "the arrow walks the deck too")
        compare(child("deckNext").text, "Get started", "the Welcome has no release notes")
        press("deckNext")
        tryCompare(modal, "visible", false)
        compare(Onboarding.asked, ["close"], "closing marks the version seen")
    }

    function test_a_cards_button_closes_the_deck_and_says_where_to_go() {
        show({ kind: "welcome", version: "0.1.0", deck: welcomeDeck })
        modal.cardIndex = 1
        waitForRendering(modal.contentItem)
        compare(child("cardAction").text, "Add a folder")
        press("cardAction")
        tryCompare(modal, "visible", false)
        compare(chosen.count, 1)
        compare(chosen.signalArguments[0][0], "libraryFolders")
        compare(Onboarding.asked, ["close"])
    }

    function test_escape_closes_from_any_card_and_marks_it_seen() {
        show({ kind: "welcome", version: "0.1.0", deck: welcomeDeck })
        keyClick(Qt.Key_Escape)
        tryCompare(modal, "visible", false)
        compare(Onboarding.asked, ["close"])
    }

    function test_whats_new_ends_in_its_release_notes() {
        show({ kind: "whatsNew", version: "0.2.0", date: "2026-11-01", hasBody: true,
               toc: [{ id: "fixes", label: "Fixes" }],
               blocks: [{ kind: 0, level: 0, text: "Fixes", id: "fixes", url: "" }, { kind: 1, level: 1, text: "One fix", id: "", url: "" }],
               deck: welcomeDeck.slice(0, 2) })
        verify(child("deckHeading").text.indexOf("What's new in v0.2.0") === 0, child("deckHeading").text)
        compare(child("deckNext").text, "Next")
        press("deckNext")
        compare(child("deckNext").text, "Release notes")
        press("deckNext")
        verify(child("releaseNotes").visible && !child("deckCard").visible)
        compare(child("deckNext").text, "Close")
        press("deckNext")
        tryCompare(modal, "visible", false)
        compare(Onboarding.asked, ["close"])
    }

    function test_whats_new_without_cards_opens_on_the_notes_and_says_when_there_are_none() {
        show({ kind: "whatsNew", version: "0.1.0", hasBody: false, loading: false, deck: [] })
        verify(child("releaseNotes").visible)
        verify(child("notesMissing").visible, "offline or unpublished, it says the notes are not available")
        verify(!child("deckBack").visible)
        compare(child("deckNext").text, "Close")
    }
}
