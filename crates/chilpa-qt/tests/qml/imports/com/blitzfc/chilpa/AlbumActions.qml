pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property int selectedCount: 0
    function sources(id, revision) { return "[]" }
    function quality(id, revision) { return '{"tier":3,"label":"FLAC 24/96"}' }
    function available(id, revision) { return true }
    function selected(id, revision) { return false }
    property double playedId: 0
    function play(id, shuffled) { playedId = id }
    function queue(id) {}
    function trackIdsJson(id) { return "[]" }
}
