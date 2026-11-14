import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

ArtTile {
    id: portrait
    property string name: ""
    property string mbid: ""
    property url chosen
    property bool activeImages: visible
    readonly property string credit: photo.credit
    readonly property string creditPage: photo.page
    source: activeImages ? chosen.toString() !== "" ? chosen : photo.source : ""
    Accessible.description: credit
    PortraitModel {
        id: photo
        objectName: "portraitResolver"
        name: portrait.name; mbid: portrait.mbid
        active: portrait.activeImages && portrait.chosen.toString() === ""
    }
    HoverHandler {
        onHoveredChanged: hovered && portrait.credit !== "" ? TipState.show(portrait.credit + "\n" + portrait.creditPage, portrait) : TipState.hide(portrait)
    }
    onActiveImagesChanged: if (!activeImages) TipState.hide(portrait)
    Component.onDestruction: TipState.hide(portrait)
}
