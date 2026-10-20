pragma Singleton
import QtQuick

// The chips bridge as the tests see it: two chips, the second kept out, and
// each edit answered with what was asked.
QtObject {
    property string chipsOf: "{\"any\":false,\"chips\":[{\"label\":\"Genre: Jazz\",\"text\":\"genre:Jazz\",\"negated\":false},{\"label\":\"Label: Verve\",\"text\":\"-label:Verve\",\"negated\":true}]}"
    function read(query) { return query === "" ? "{\"any\":false,\"chips\":[]}" : chipsOf }
    function without(query, at) { return "without:" + at }
    function flipped(query, at) { return "flipped:" + at }
    function joined(query, any) { return "joined:" + any }
    function excluding(query, column, value) { return "excluding:" + column + ":" + value }
}
