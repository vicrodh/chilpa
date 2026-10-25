pragma Singleton
import QtQuick
QtObject {
    property string origin: "fixture-origin"
    property bool live: false
    property string path: "/test.wav"
    property int queueRev: 0
    property bool playing: true
    // The desktop asks for the window: 0 to the front, 1 quit, 2 toggle.
    signal windowAsked(int what)
    function quit() { windowAsked(1) }
    property bool paused: false
    property bool gapless: true
    property bool normalize: false
    property bool normalizeApplies: false
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
    property string rejectedLyrics: ""
    function rejectLyrics(revision) { rejectedLyrics = revision }
    property string changedLyrics: ""
    function changeLyricsSource(revision) { changedLyrics = revision }
    function seek(seconds) { sought = seconds; position = seconds }
    property bool shuffle: false
    property int repeat: 0
    property int volume: 50
    property bool muted: false
    property bool volumeLocked: false
    property string volumeNote: ""
    property int toggles: 0
    property int nexts: 0
    property int previouses: 0
    function toggle() { toggles += 1; paused = !paused }
    function next() { nexts += 1 }
    function previous() { previouses += 1 }
    function toggleShuffle() { shuffle = !shuffle }
    function cycleRepeat() { repeat = (repeat + 1) % 3 }
    function chooseVolume(percent) { volume = percent }
    property int volumeSaves: 0
    function persistVolume() { volumeSaves += 1 }
    function chooseMuted(on) { muted = on }
    function nudge(seconds) { seek(position + seconds) }
    function clock(seconds) { return String(Math.floor(seconds)) }
}
