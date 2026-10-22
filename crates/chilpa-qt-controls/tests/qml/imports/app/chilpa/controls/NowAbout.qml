pragma Singleton
import QtQuick
QtObject {
    // What the facade answers for the track and the record that play.
    property var trackAnswer: ({})
    property var albumAnswer: ({})
    function track(path, rev) { return JSON.stringify(trackAnswer) }
    function album(path, rev) { return JSON.stringify(albumAnswer) }
}
