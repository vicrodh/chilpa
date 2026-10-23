pragma Singleton
import QtQuick

// The deck's bridge as the tests see it: it keeps what it was asked, and the
// test sets the document.
QtObject {
    property string doc: "{}"
    property var asked: []
    property int refreshed: 0
    function command(action) { asked = asked.concat([action]) }
    function refresh() { refreshed += 1 }
}
