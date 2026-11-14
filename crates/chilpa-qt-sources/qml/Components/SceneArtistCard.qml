import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

ArtistCard {
    id: card
    chosenArt: art.toString() !== "" ? art : artist.collection === true ? artist.portrait || "" : firstSource.local ? firstSource.image || "" : firstSource.plugin && firstSource.image
        ? Sources.art(firstSource.plugin, firstSource.image, Library.remoteArtRev) : ""
}
