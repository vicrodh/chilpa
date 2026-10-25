pragma Singleton
import QtQuick
QtObject {
    // What the facade answers for the track and the record that play.
    property var trackAnswer: ({})
    property var albumAnswer: ({})
    property var calls: ({track: 0, album: 0})
    property var retried: ({})
    property bool allowRetry: true
    function track(path, rev) { calls.track++; return JSON.stringify(trackAnswer) }
    function album(path, rev) { calls.album++; return JSON.stringify(albumAnswer) }
    property string refetchedPath: ""
    property bool allowRefetch: true
    function refetchAlbum(path) {
        refetchedPath = path
        if (!allowRefetch) return false
        albumAnswer = Object.assign({}, albumAnswer, {loading:true})
        return true
    }
    function retry(part, path) {
        retried = ({part: part, path: path})
        if (!allowRetry) return false
        if (part === "track") trackAnswer = ({loading: true})
        else albumAnswer = ({loading: true})
        return true
    }
}
