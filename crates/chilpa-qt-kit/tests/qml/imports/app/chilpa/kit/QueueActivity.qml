pragma Singleton
import QtQuick
QtObject {
    function collectionMenu() { return '[{"act":"next","text":"Play next"}]' }
    property bool busy: false
    property bool pending: false
    property int refreshes: 0
    function refresh() { ++refreshes; busy = pending }
    property string offer: ""
    property var answers: []
    function answer(choice) { answers = answers.concat([choice]); offer = "" }
}
