import QtQuick
import app.chilpa

// I share the album grid's pointer actions and source badge presentation.
QtObject {
    property Item grid
    function clicked(at, albumId, title, modifiers, available) {
        if (!available)
            AlbumActions.explain(albumId)
        else if (!grid.isRound(albumId) && (modifiers !== 0 || AlbumActions.selectedCount > 0))
            grid.model.select(at, modifiers === 0 ? 1 : modifiers)
        else if (!grid.isRound(albumId) && !grid.list && SettingsModel.quickViewClick)
            toggleQuick(at)
        else {
            grid.model.prepareOpen(at)
            grid.open(albumId, title)
        }
    }

    function toggleQuick(at) {
        grid.model.prepareOpen(at)
        grid.model.expanded = grid.model.expanded === at ? -1 : at
    }

    function badges(albumId, rev) {
        if (albumId === 0)
            return []
        let found = JSON.parse(AlbumActions.sources(albumId, rev) || "[]")
        if (albumId > 0 && found.length === 0)
            found = [{ local: true }]
        if (albumId > 0 && !(grid.model && grid.model.mixed) && !found.some(s => !s.local))
            return []
        return found.map(s => s.local && !s.icon ? Object.assign({}, s, { icon: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png" }) : s)
            .filter((s, at, all) => all.findIndex(o => o.icon === s.icon) === at)
    }
}
