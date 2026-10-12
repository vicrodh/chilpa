import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The location and seed artist shown above each catalog's QBZ-style grid.
Item {
    id: hero
    property string title: ""
    property string artistName: ""
    property string genres: ""
    property string flagSource: ""
    height: Metrics.sceneHeroHeight
    Row {
        width: parent.width; spacing: Metrics.modalInset
        ArtTile {
            id: flag
            width: Metrics.sceneHeroFlag; height: width
            radius: width * Metrics.sceneCircleRadius; edged: false
            source: hero.flagSource; visible: hero.flagSource !== ""
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
            width: parent.width - (flag.visible ? Metrics.sceneHeroHeight : 0)
            Text { width: parent.width; text: hero.title; elide: Text.ElideRight; font.pointSize: Type.title; font.weight: Type.bold; color: Theme.color.textPrimary }
            Text { width: parent.width; text: I18n.tr("Based on {artist} · {genres}", I18n.rev).replace("{artist}", hero.artistName).replace("{genres}", hero.genres); wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
    }
}
