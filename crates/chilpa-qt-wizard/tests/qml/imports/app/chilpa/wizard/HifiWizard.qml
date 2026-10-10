pragma Singleton
import QtQuick

// The wizard bridge as the tests see it: it keeps what it was asked, and
// the test sets the document.
QtObject {
    property string doc: "{}"
    property var asked: []
    property int refreshed: 0
    function command(action, argument) { asked = asked.concat([action + ":" + argument]) }
    function refresh() { refreshed += 1 }
}
