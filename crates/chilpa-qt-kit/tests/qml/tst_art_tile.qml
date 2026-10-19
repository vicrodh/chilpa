import QtQuick
import QtTest
import "../../qml/Primitives"

// A cover has to be seen on every renderer. This suite runs on Qt's software
// rasterizer, which draws no shader: the rounded pass shows nothing there.
TestCase {
    name: "ArtTile"
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

    function test_a_cover_is_drawn_without_shaders() {
        compare(tile.GraphicsInfo.api, GraphicsInfo.Software, "the suite runs where no shader draws")
        waitForRendering(picture)
        tile.source = picture.toDataURL("image/png")
        tryVerify(() => tile.ready, 2000, "the cover loads")
        tryVerify(() => grabImage(stage).green(60, 60) > 200, 1000, "the cover is on screen: " + grabImage(stage).green(60, 60))
    }
}
