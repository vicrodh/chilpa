pragma Singleton
import QtQuick
QtObject {
    // What the facade answers for the track and the record asked for: the
    // answer kept for that path, else the default one.
    property var trackAnswer: ({})
    property var albumAnswer: ({})
    property var trackAnswers: ({})
    property var calls: ({track: 0, album: 0})
    property var retried: ({})
    property bool allowRetry: true
    property string askedPath: ""
    signal trackInfoRequested(string path, string title)
    function track(path, rev) { calls.track++; askedPath = path; return JSON.stringify(trackAnswers[path] || trackAnswer) }
    function album(path, rev) { calls.album++; askedPath = path; return JSON.stringify(albumAnswer) }
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
