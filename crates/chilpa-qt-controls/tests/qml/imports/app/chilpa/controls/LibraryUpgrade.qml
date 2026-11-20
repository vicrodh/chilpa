pragma Singleton
import QtQuick
// The facade's library upgrade as the startup cover reads it
// (`library_upgrade_view`), and what the cover asks of it.
QtObject {
    property var state: ({ active: false })
    readonly property string snapshot: JSON.stringify(state)
    property int refreshed: 0
    property int retried: 0
    property int left: 0
    property int restored: 0
    function refresh() { refreshed++ }
    function retry() { retried++ }
    function quit() { left++ }
    function restore() { restored++ }
}
