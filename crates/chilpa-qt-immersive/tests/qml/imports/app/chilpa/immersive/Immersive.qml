pragma Singleton
import QtQuick

// The immersive view's bridge as the tests see it.
QtObject {
    property bool open: false
    property int viewMode: 1
    property int mode: 0
    property int splitPanel: 0
    property int scene: 0
    property bool accelerated: true
    property string searchAction: "replace"
    property bool devOpen: false
    property int barBottom: 16
    property real barAlpha: 0.8
    property bool barBlur: false
    property bool markable: true
    property bool favorite: false
    property int likes: 0
    // What the next key comes to, and what was asked.
    property string answer: ""
    property var views: []
    property var scenes: []
    property int cycles: 0
    function toggle() { open = !open }
    function openFromMenu() { mode = 0; open = true }
    function openView(viewMode, mode, splitPanel) { open = true; setView(viewMode, mode, splitPanel) }
    function setView(viewMode, mode, splitPanel) { this.viewMode = viewMode; this.mode = mode; this.splitPanel = splitPanel; views.push([viewMode, mode, splitPanel]) }
    function chooseScene(scene) { this.scene = scene; scenes.push(scene) }
    function setAvailable(value) { accelerated = value; if (!value) { scene = 0; if (mode === 7 || mode === 8) mode = 3 } }
    function cycleScene() { cycles += 1; scene = scene === 0 ? 1 : 0 }
    function keyPressed(key, modifiers, text, typing) { return answer }
    function reload() {}
    function likePlaying() { likes += 1; favorite = !favorite }
    function refreshFavorite() {}
    property bool favoritePending: false
}
