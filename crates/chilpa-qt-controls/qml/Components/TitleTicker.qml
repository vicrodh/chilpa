import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A line cut short slides to show its end while the pointer rests on it,
// besides saying itself whole in the tooltip. It is the line the window's
// tooltip is about, so one at a time: a copy slides over the line, which
// hides meanwhile. Nothing is built and nothing runs while no line is held.
Item {
    id: ticker

    readonly property bool on: Prefs.rev >= 0 && SettingsModel.kept("appearance.titleTicker") !== "false"
    readonly property Item held: TipState.target
    // A title with a badge is cut in its own line of text, not in what holds both.
    readonly property Item line: !on || !enabled || Motion.reduced || !held || held.truncated !== true ? null
        : held.advisoryWidth !== undefined ? held.children[0] : held

    readonly property Item sliding: line !== null && line.wrapMode === Text.NoWrap ? line : null
    // The line hides under its copy and is given back as it was, its own bindings included.
    property Item hidden: null
    onSlidingChanged: { hide.when = false; hidden = sliding; hide.when = sliding !== null }
    Binding { id: hide; target: ticker.hidden; property: "opacity"; value: 0; when: false; restoreMode: Binding.RestoreBindingOrValue }

    Loader {
        active: ticker.sliding !== null
        sourceComponent: Item {
            id: slide
            objectName: "titleTickerSlide"
            readonly property Item line: ticker.sliding
            // Read again with every step, so the copy follows a list that moves under the pointer.
            readonly property point at: line ? (words.x, line.mapToItem(parent, 0, 0)) : Qt.point(0, 0)

            parent: ticker.Overlay.overlay
            z: Metrics.tooltipLayer - 1
            x: at.x
            y: at.y
            width: line ? line.width : 0
            height: line ? line.height : 0
            clip: true

            Text {
                id: words
                objectName: "titleTickerWords"
                readonly property real over: Math.max(0, implicitWidth - slide.width)
                height: slide.height
                text: slide.line ? slide.line.text : ""
                font: slide.line ? slide.line.font : Qt.font({})
                color: slide.line ? slide.line.color : Theme.color.textPrimary
                verticalAlignment: slide.line ? slide.line.verticalAlignment : Text.AlignTop
                // As the line draws it: a credit with links is marked up, and plain would show the markup.
                textFormat: slide.line ? slide.line.textFormat : Text.PlainText
                linkColor: slide.line ? slide.line.linkColor : color
                SequentialAnimation on x {
                    loops: Animation.Infinite
                    running: words.over > 0
                    PauseAnimation { duration: Motion.tickerRest }
                    NumberAnimation { to: -words.over; duration: words.over * Motion.tickerPerPixel }
                    PauseAnimation { duration: Motion.tickerRest }
                    NumberAnimation { to: 0; duration: Motion.page }
                }
            }
        }
    }
}
