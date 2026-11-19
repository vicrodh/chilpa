import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A tile that loses its cover shows its placeholder and keeps no hidden copy
// of the old picture while the next one loads. The check lives here because
// the kit's own suite has no room left under its line cap.
TestCase {
    name: "ArtTileRelease"
    when: windowShown
    visible: true
    width: 240
    height: 160

    Rectangle {
        id: stage
        anchors.fill: parent
        color: "black"
        ArtTile { id: tile; x: 10; y: 10; width: 100; height: 100; edged: false }
    }
    Canvas {
        id: picture
        x: 150; y: 10; width: 64; height: 64
        onPaint: {
            const context = getContext("2d")
            context.fillStyle = "#00ff00"
            context.fillRect(0, 0, width, height)
        }
    }

    function test_a_cleared_cover_leaves_the_placeholder() {
        compare(findChild(tile, "artImage").retainWhileLoading, false,
                "a hidden old cover is not retained during loading")
        waitForRendering(picture)
        tile.source = picture.toDataURL("image/png")
        tryVerify(() => tile.ready, 2000, "the cover loads")
        tryVerify(() => grabImage(stage).green(60, 60) > 200, 1000, "the cover is on screen")
        tile.source = ""
        tryCompare(tile, "ready", false)
        waitForRendering(tile)
        verify(grabImage(stage).green(60, 60) < 200, "the placeholder replaces the old cover")
    }
}
