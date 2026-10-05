import QtQuick
import com.blitzfc.chilpa
import "../Theme"

Text {
    id: line
    objectName: "albumArtistLine"
    property double albumId: 0
    property var document: albumId ? JSON.parse(AlbumActions.information(albumId, Library.contentRev + Library.remoteRev) || "{}") : ({})
    signal followed()
    text: document.artist_html || ""
    textFormat: Text.StyledText
    wrapMode: Text.WordWrap
    font.pointSize: Type.body
    color: Theme.color.textSecondary
    linkColor: hoveredLink ? Theme.color.accentHover : color
    onLinkActivated: link => {
        const artist = (document.artist_links || [])[Number(link)]
        if (artist) { followed(); Navigation.goKey(artist.target, artist.key) }
    }
    HoverHandler { cursorShape: line.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
