import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/artwork/qml/Components"

// A composer's picture menu, as an artist's: add, change or remove the
// picture the listener chose, view it whole and save it. A composer and an
// artist of the same name share the one picture chosen for that name.
Item {
    id: host
    property string name: ""
    property url picture
    function view(picture) { lightbox.active = true; lightbox.item.show(picture) }
    // Opens at `x`, `y` of this item, with the rows of now.
    function popupAt(name, picture, x, y) {
        host.name = name; host.picture = picture
        menu.active = true
        menu.item.custom = Artwork.artist(name, Artwork.rev).toString() !== ""
        menu.item.showing = picture.toString() !== ""
        menu.item.popupAt(x, y)
    }
    Loader {
        id: menu
        active: false
        sourceComponent: Component { CoverMenu {
            objectName: "composerPictureMenu"
            parent: host
            kind: "artist"
            onChoose: Artwork.chooseArtist(host.name)
            onForget: Artwork.forgetArtist(host.name)
            onView: host.view(host.picture)
            onSave: Artwork.saveArtist(host.name, host.picture)
        } }
    }
    Loader { id: lightbox; active: false; sourceComponent: Component { CoverLightbox {} } }
}
