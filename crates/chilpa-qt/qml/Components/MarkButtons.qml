import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The listener's marks on what a page is about: the heart that makes it a
// favorite and, where it takes stars, the button that rates it.
Row {
    id: marks

    // "album", "playlist", "artist" or "track".
    property string kind
    // Its id, or an artist's name.
    property string key
    property bool rates: true
    property bool contrasted: false

    readonly property bool favorite: key !== "" && Ratings.favorite(kind, key, Ratings.rev)

    spacing: Metrics.gap8
    visible: key !== "" && key !== "0"

    IconButton {
        visible: marks.kind === "artist" || Number(marks.key) > 0
        round: true
        glyph: marks.favorite ? Icons.heartFilled : Icons.heart
        filled: marks.favorite
        checked: marks.favorite
        heart: true
        tone: marks.contrasted ? "overlay" : marks.favorite ? "accent" : "secondary"
        tip: marks.favorite ? I18n.tr("Remove from favorites", I18n.rev)
                            : I18n.tr("Add to favorites", I18n.rev)
        onClicked: Ratings.setFavorite(marks.kind, marks.key, !marks.favorite)
    }
    RatingButton {
        visible: marks.rates && Number.isFinite(target) && target !== 0
        kind: marks.kind
        target: marks.rates ? Number(marks.key) : 0
        contrasted: marks.contrasted
    }
}
