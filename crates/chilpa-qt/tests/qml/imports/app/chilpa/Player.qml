pragma Singleton
import QtQuick
QtObject {
    property string path: "/test.wav"
    property int queueRev: 0
    property bool playing: true
    // The desktop asks for the window: 0 to the front, 1 quit, 2 toggle.
    signal windowAsked(int what)
    function quit() { windowAsked(1) }
    property bool paused: false
    property bool bitPerfect: false
    property string title: "Fixture track"
    property string artist: "Fixture artist"
    property string album: "Fixture album"
    property int albumId: 42
    property url art: "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect width="48" height="48" fill="#a3161f"/><circle cx="24" cy="24" r="14" fill="#31363b"/></svg>')
    property int qualityTier: 2
    property string qualityLong: "FLAC · 16-bit / 44.1 kHz"
    property string outputFormat: "PCM"
    property string device: "Local output"
    property string route: "Direct"
    property string elapsed: "0:09"
    property string remaining: "-0:21"
    property real position: 9
    property real duration: 30
    property real sought: -1
    property var lyrics: []
    property string lyricsJson: ""
    property bool lyricsSynced: false
    property int lyricsLine: -1
    function seek(seconds) { sought = seconds; position = seconds }
    function clock(seconds) { return String(Math.floor(seconds)) }
}
