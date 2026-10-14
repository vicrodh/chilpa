import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/TextLinks.js" as TextLinks

Text {
    id: line
    objectName: "albumArtistLine"
    property double albumId: 0
    property var document: albumId ? JSON.parse(AlbumActions.artistInformation(albumId, Library.contentRev + Library.remoteRev) || "{}") : ({})
    property bool contrasted: false
    readonly property color restingColor: contrasted ? Theme.color.textOnAccentMuted : color
    readonly property color activeColor: contrasted ? color : Theme.color.accentHover
    signal followed()
    text: TextLinks.render(document.artist_html, hoveredLink, restingColor, activeColor)
    textFormat: Text.RichText
    wrapMode: Text.WordWrap
    font.pointSize: Type.body
    color: Theme.color.textSecondary
    linkColor: restingColor
    onLinkActivated: link => {
        const artist = (document.artist_links || [])[Number(link)]
        if (artist) { followed(); Navigation.goKey(artist.target, artist.key) }
    }
    HoverHandler { cursorShape: line.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
