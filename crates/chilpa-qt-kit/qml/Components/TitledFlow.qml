import QtQuick
import "../Theme"

// A titled group of pieces in the Gallery.
Column {
    id: section

    property string title
    default property alias pieces: body.data

    spacing: Metrics.gap12

    Text {
        text: section.title
        font.pointSize: Type.section
        font.weight: Type.heading
        color: Theme.color.textPrimary
    }

    Flow {
        id: body
        width: section.width
        spacing: Metrics.gridGap
    }
}
