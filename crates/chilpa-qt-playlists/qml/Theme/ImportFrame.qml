pragma Singleton
import QtQuick

// The playlist importer's numbers: its column is 760 wide at most, as a
// readable form with room for a song and its match on one line; a song's
// number takes 36, room for four digits; and the importer sends its rows
// 200 at a time.
QtObject {
    readonly property int column: 760
    readonly property int number: 36
    readonly property int page: 200
}
