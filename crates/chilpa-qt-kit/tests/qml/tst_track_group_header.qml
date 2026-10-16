import QtQuick
import QtTest
import "../../qml/Theme"
import "../../qml/Components"

TestCase {
    name: "TrackGroupHeader"
    when: windowShown
    visible: true
    width: 500
    height: 120

    TrackGroupHeader {
        id: header
        width: 480
        height: implicitHeight
        work: true
        label: "Das Rheingold, Scene 1"
        composer: "Richard Wagner"
        composerTarget: "artist:Richard Wagner"
    }

    function test_work_header_has_the_qbz_dimensions_and_composer() {
        compare(header.implicitHeight, 40)
        compare(header.composerLabel.text, "Richard Wagner")
        verify(header.composerLabel.visible)
        compare(header.composerLabel.color.toString(), Theme.color.accent.toString())
    }
}
