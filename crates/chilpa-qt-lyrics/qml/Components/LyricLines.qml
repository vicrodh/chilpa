import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

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
    // The moment of the track and where the sung line ends.
    property real nowMs: 0
    property real sungEnd: 0
    // The translation of each line by its place, or null when none is shown.
    property var translation: null
    property real size: Type.lyric
    property color sungColor: Theme.color.textPrimary
    property string dimming: "strong"
    property bool uppercase: false
    property bool lite: false

    signal sought(real ms)

    readonly property bool following: synced && follow
    // The line that was being sung before this one.
    property int previous: -1
    // Half the view above the first line and under the last: enough for
    // either to be sung in the middle, however many rows it wraps to.
    readonly property real room: following ? height / 2 : Metrics.gap10

    // Puts the sung line in the middle: gliding there from a line or two
    // away, at once from further or when the lyrics have just come.
    function center() {
        const near = previous >= 0 && Math.abs(current - previous) <= 2
        previous = current
        if (!following || current < 0)
            return
        const from = contentY
        glide.stop()
        positionViewAtIndex(current, ListView.Center)
        if (!near)
            return
        glide.from = from
        glide.to = contentY
        glide.start()
    }
    // Once the lines have taken the change in: the sung one is drawn anew.
    onCurrentChanged: Qt.callLater(center)
    onFollowingChanged: Qt.callLater(center)
    onHeightChanged: Qt.callLater(center)
    onLinesChanged: {
        previous = -1
        Qt.callLater(center)
    }

    NumberAnimation {
        id: glide
        target: view
        property: "contentY"
        duration: Motion.lyricScroll
        easing.type: Easing.InOutQuad
    }

    clip: true
    model: lines
    spacing: Metrics.gap12
    header: Item { height: view.room }
    footer: Item { height: view.room }
    boundsBehavior: Flickable.StopAtBounds

    delegate: LyricLine {
        required property int index
        required property var modelData
        width: ListView.view.width
        line: modelData
        synced: view.synced
        position: index
        current: view.current
        nowMs: sung ? view.nowMs : 0
        endMs: sung ? view.sungEnd : 0
        translation: view.translation ? (view.translation[index] || "") : ""
        size: view.size
        sungColor: view.sungColor
        dimming: view.dimming
        uppercase: view.uppercase
        wordByWord: !view.lite
        onSought: (ms) => view.sought(ms)
    }
}
