import QtQuick
import app.chilpa
import app.chilpa.kit

Item {
    id: writer
    property bool pending: false
    property string sourceId: ""
    property string request: ""
    signal finished(string message, bool warning)
    function submit(source, id, name, description, tracks) {
        if (pending) return
        sourceId = source
        request = String(Date.now()) + ":playlist"
        pending = Playlists.writeSource(source, id, name, description, tracks, request)
        if (!pending) finished(I18n.tr("This source cannot accept these tracks right now.", I18n.rev), true)
    }
    Timer {
        interval: 250; repeat: true; running: writer.pending
        onTriggered: {
            const state = JSON.parse(Playlists.writeState(writer.sourceId) || "null") || {}
            if (!state.request_id) {
                writer.pending = false
                writer.finished(I18n.tr("The source disconnected. Check the playlist before retrying.", I18n.rev), true)
                return
            }
            if (state.request_id !== writer.request || state.pending) return
            writer.pending = false
            const warning = !!state.error || !!(state.result || {}).warning
            writer.finished(state.error || (state.result || {}).warning || I18n.tr("Saved", I18n.rev), warning)
        }
    }
}
