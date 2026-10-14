import QtQuick
import QtTest
import app.chilpa
import "../../../chilpa-qt/qml/Patterns"

// The lyrics flyout (a Pattern of the window, tested from here so the
// window stays under its line budget) mirrors the "Translate lyrics" toggle of Settings:
// both keep the same preference, and the flyout reads it again each time
// it opens.
TestCase {
    name: "LyricsOptions"
    when: windowShown
    visible: true
    width: 600
    height: 700

    // The window's settings, as the test sees them.
    QtObject {
        id: fake
        property var preferences: ({})
        function kept(key) { return preferences[key] || "" }
        function keep(key, value) { preferences[key] = value }
    }

    LyricsOptions { id: flyout; store: fake }

    function test_translating_is_kept_where_settings_keeps_it() {
        fake.preferences = ({})
        flyout.open()
        tryCompare(flyout, "opened", true)
        const row = findChild(flyout.contentItem, "translateRow")
        verify(row !== null, "the flyout mirrors the toggle of Settings")
        compare(row.checked, false, "off until asked for")
        row.toggled(true)
        compare(fake.kept("integrations.lyrics.translate"), "true")
        compare(row.checked, true)
        flyout.close()
    }

    function test_the_mirror_reads_what_settings_kept_when_it_opens() {
        fake.preferences = ({ "integrations.lyrics.translate": "true" })
        flyout.open()
        tryCompare(flyout, "opened", true)
        compare(findChild(flyout.contentItem, "translateRow").checked, true, "what Settings turned on shows on")
        flyout.close()
    }
}
