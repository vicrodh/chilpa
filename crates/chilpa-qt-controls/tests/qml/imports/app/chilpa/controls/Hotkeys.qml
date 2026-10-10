pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    // What the next key comes to, and what was asked last.
    property string answer: ""
    property var commands: []
    property int liked: 0
    signal searchAsked()
    function refresh() {}
    // A modifier pressed alone is no key, as in the facade.
    function pressed(key, modifiers, text, typing) { return key >= Qt.Key_Shift && key <= Qt.Key_Alt ? "" : answer }
    property string scopeAsked: ""
    function pressedIn(scope, key, modifiers, text, typing) { scopeAsked = scope; return pressed(key, modifiers, text, typing) }
    function command(action, argument) { commands = commands.concat([action + ":" + argument]) }
    function likePlaying() { ++liked }
    function shortcut(id) { return id === "nav.quit" ? "Ctrl + Q" : "" }
    signal asked(string action)
    function ask(action) { asked(action) }
}
