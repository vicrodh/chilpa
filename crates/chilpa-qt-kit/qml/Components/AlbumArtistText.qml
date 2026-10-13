import QtQuick
import app.chilpa
import app.chilpa.sources
import "../Theme"

// I keep the card cheap until its artist credit is followed.
Text {
    id: label
    objectName: "albumCardArtist"
    property double albumId: 0
    property string artist: text
    property var destination: ({})
    property bool navigable: true
    signal fallback()
    elide: Text.ElideRight
    textFormat: Text.PlainText
    font.pointSize: Type.meta
    color: hover.hovered && navigable ? Theme.color.accentHover : Theme.color.textSecondary
    function follow() {
        const route = destination.key ? destination : JSON.parse(Sources.artistLink(albumId, artist) || "{}")
        if (route.key) Navigation.goKey(route.target, route.key)
        else fallback()
    }
    HoverHandler { id: hover; cursorShape: label.navigable ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { enabled: label.navigable; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: label.follow() }
}
