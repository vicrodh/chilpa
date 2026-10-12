import QtQuick
import QtTest
import app.chilpa
import app.chilpa.waveform
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

TestCase {
    name: "PlayerBand"
    when: windowShown
    visible: true
    width: 1200
    height: 300

    PlayerBar { id: bar; width: 1100; y: 100 }

    function test_the_waveform_sits_centered_between_the_top_edge_and_the_song_card() {
        Waveform.open = true
        tryCompare(bar, "tall", true)
        const band = findChild(bar, "playerBand")
        // The song card: the one piece of the bar with a tall layout and its times.
        const card = band.parent.children.find(c => c.tall !== undefined && c.showTimes !== undefined)
        verify(card !== undefined)
        verify(band.visible)
        const above = band.y
        const below = card.y - (band.y + band.height)
        verify(Math.abs(above - below) <= 1, "as much room above the band as under it: " + above + " and " + below)
        Waveform.open = false
    }

    // A bar at the window's top grows downward: the band under the song card,
    // and the caret pointing the way it folds.
    function test_at_the_top_the_band_opens_under_the_bar_and_the_caret_says_so() {
        PlayerLayout.mode = "top"
        Waveform.open = true
        tryCompare(bar, "tall", true)
        const band = findChild(bar, "playerBand")
        const card = band.parent.children.find(c => c.tall !== undefined && c.showTimes !== undefined)
        verify(band.y >= card.y + card.height, "under the song card: " + band.y + " and " + (card.y + card.height))
        const caret = findChild(bar, "playerUnfold")
        compare(caret.glyph, Icons.chevronUp, "open at the top, it folds up")
        Waveform.open = false
        compare(caret.glyph, Icons.chevronDown, "and unfolds down")
        PlayerLayout.mode = "floating"
        compare(caret.glyph, Icons.chevronUp, "at the bottom it unfolds up, as before")
    }
}
