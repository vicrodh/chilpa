import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Primitives"
import "../Components"
import "../Patterns"
import "../Theme"

// The mini player's pieces: the footer at its three scales, the capsule
// unfolded, the card, and each surface and control on its own. The window
// is listed without being shown; the host is a screen of the main window.
TitledFlow {
    title: "Mini player"
    readonly property int cardWidth: Card.windowSpan - Card.gutter * 2 // derived: the gutters

    MiniShell { width: cardWidth; height: Card.heightExpanded - Card.gutter * 2 } // derived: the gutters
    MiniWindow { visible: false }
    MiniFooter { mode: 0; width: cardWidth; height: Card.footerFull; cardRadius: Card.radiusExpanded }
    MiniFooter { mode: 1; width: cardWidth; height: Card.footerCompact; cardRadius: Card.radiusCondensed }
    MiniFooter { mode: 2; width: cardWidth; height: Card.footerMicro; cardRadius: Card.radiusCondensed }
    MiniFooterBand { width: cardWidth; height: Card.footerFull; hasTrack: true; canPlay: true }
    MiniFooterMicro { width: cardWidth; height: Card.footerMicro; hasTrack: true; canPlay: true }
    MiniCompactSurface { width: cardWidth; height: Card.heightCompact - Card.footerCompact }
    MiniArtworkSurface { width: cardWidth; height: Card.heightExpanded - Card.footerFull }
    MiniQueueSurface { width: cardWidth; height: Card.heightCompact }
    MiniLyricsSurface { width: cardWidth; height: Card.heightCompact }
    MiniCapsule { expanded: true }
    MiniVolume {}
    MiniTransport {}
    MiniSeek { width: cardWidth; height: Card.seekHeight[0]; progress: 0.4 }
    MiniCover { width: Card.coverMin; height: Card.coverMin }
    TransportButton { glyph: Icons.playerPlayFilled; filled: true }
    CapsuleButton { glyph: Icons.square }
}
