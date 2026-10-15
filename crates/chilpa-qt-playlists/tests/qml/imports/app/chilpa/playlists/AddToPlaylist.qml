pragma Singleton
import QtQuick

QtObject {
    property string lastAsked: ""
    signal asked(string tracks)
    function finish() {}
    function show(tracks) { lastAsked = tracks; asked(tracks) }
}
