import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

// The waveform seek at the player bar's width, with a loop marked on it;
// the bars come from whatever plays, so without a track only the rail
// shows.
Rectangle {
    id: gallery
    color: Theme.color.surfaceLcd
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the sample and its margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the sample and its margins
    readonly property int sampleWide: 900

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.contentX
        WaveformSeek { width: gallery.sampleWide; height: Metrics.waveformBand; position: gallery.sampleWide / 4; duration: gallery.sampleWide; bubbleText: "1:10" } // derived: a quarter of the way
        WaveformSeek { width: gallery.sampleWide / 2; height: Metrics.waveformBand; position: gallery.sampleWide / 8; duration: gallery.sampleWide; loopStart: gallery.sampleWide / 4; loopEnd: gallery.sampleWide / 2 } // derived: half the sample, a loop over its second quarter
    }
}
