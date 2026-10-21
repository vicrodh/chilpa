import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The lines of the lyrics. While they follow the music the line being sung
// is kept in the middle, with the room above the first line and under the
// last for either to get there, and the lines around it fade. A move of a
// line or two is followed smoothly; a jump lands at once.
ListView {
    id: view

    // The lines as the lyrics document has them, or only their texts.
    property var lines: []
    property bool synced: false
    // The place of the line being sung, or -1.
    property int current: -1
    property bool follow: true
    readonly property bool live: visible && (!Window.window || (Window.window.visibility !== Window.Hidden && Window.window.visibility !== Window.Minimized))
    // The moment of the track and where the sung line ends.
    property real nowMs: 0
    property real sungEnd: 0
    // The translation of each line by its place, or null when none is shown.
    property var translation: null
    property real size: Type.lyric
    property color sungColor: Theme.color.textPrimary
    property color dimColor: Theme.color.textSecondary
    property color pastColor: Theme.color.textTertiary
    property string dimming: "strong"
    property bool uppercase: false
    property bool lite: false

    signal sought(real ms)
    signal followAsked(bool on)

    readonly property bool following: synced && follow
    // Half the view above the first line and under the last: enough for
    // either to be sung in the middle, however many rows it wraps to.
    readonly property real room: synced ? height / 2 : Metrics.gap10

    // Puts the sung line in the middle: gliding there from a line or two
    // away, at once from further or when the lyrics have just come.
    function center() { tracker.request(false) }
    onLinesChanged: tracker.reset()
    onTranslationChanged: tracker.request(false)
    onSizeChanged: tracker.request(false)
    onUppercaseChanged: tracker.request(false)
    LyricTracker {
        id: tracker
        view: view
        scrollBar: view.ScrollBar.vertical
        current: view.current
        following: view.following
        live: view.live
        onBrowseRequested: { view.followAsked(false); if (view.follow) view.follow = false }
    }

    clip: true
    model: lines
    spacing: Metrics.gap12
    header: Item { height: view.room }
    footer: Item { height: view.room }
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}
    PillButton {
        objectName: "lyricsFollow"
        // I keep the button outside Flickable's press filter during inertia.
        parent: view.parent
        x: view.x + (view.width - width) / 2 // derived: centered over this viewport
        y: view.y + view.height - height - Metrics.gap8
        z: view.z + 1
        visible: view.visible && view.synced && view.count > 0 && !view.follow
        text: I18n.tr("Follow lyrics", I18n.rev)
        onClicked: { view.followAsked(true); if (!view.follow) view.follow = true }
    }

    delegate: LyricLine {
        required property int index
        required property var modelData
        width: ListView.view.width
        line: modelData
        synced: view.synced
        position: index
        current: view.live ? view.current : -1
        nowMs: sung && view.live ? view.nowMs : 0
        endMs: sung ? view.sungEnd : 0
        translation: view.translation ? (view.translation[index] || "") : ""
        size: view.size
        sungColor: view.sungColor
        dimColor: view.dimColor
        pastColor: view.pastColor
        dimming: view.dimming
        uppercase: view.uppercase
        wordByWord: !view.lite
        onHeightChanged: tracker.request(false)
        onYChanged: tracker.request(false)
        onSought: (ms) => view.sought(ms)
    }
}
