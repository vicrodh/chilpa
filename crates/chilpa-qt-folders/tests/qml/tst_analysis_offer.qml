import QtQuick
import QtTest
import app.chilpa.folders
import "../../qml/Patterns"

// A folder that joins the library asks whether to analyze it too, once
// its scan began, and only when no analysis runs or waits already.
TestCase {
    name: "AnalysisOffer"
    when: windowShown
    visible: true
    width: 900
    height: 700

    AnalysisOffer { id: offer }

    function test_a_new_folder_offers_the_analysis_and_analyze_asks_for_it() {
        LibraryFolders.analyzeAsked = 0
        LibraryFolders.added()
        tryCompare(offer, "opened", true)
        const buttons = []
        const walk = item => { for (const child of item.children) { if (child.text !== undefined && child.clicked) buttons.push(child); walk(child) } }
        walk(offer.contentItem)
        verify(buttons.some(button => button.text === "Not now"), "the way out says what it is")
        buttons.find(button => button.text === "Analyze").clicked()
        tryCompare(offer, "opened", false)
        compare(LibraryFolders.analyzeAsked, 1)
    }

    function test_not_now_asks_for_nothing() {
        LibraryFolders.analyzeAsked = 0
        LibraryFolders.added()
        tryCompare(offer, "opened", true)
        keyClick(Qt.Key_Escape)
        tryCompare(offer, "opened", false)
        compare(LibraryFolders.analyzeAsked, 0)
    }

    function test_no_offer_while_an_analysis_runs_or_waits() {
        LibraryFolders.busy = true
        LibraryFolders.added()
        wait(50)
        verify(!offer.opened)
        LibraryFolders.busy = false
    }
}
