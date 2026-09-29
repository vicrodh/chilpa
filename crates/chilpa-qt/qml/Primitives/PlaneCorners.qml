import QtQuick
import "../Theme"

// The round top corners of a plane whose content reaches its edges: the
// content area, under a record's atmosphere or whatever else paints it from
// side to side. The content is not rendered into a layer to be masked, which
// would cost a pass over everything in it and its text its sharpness: each
// corner is covered with what is behind the plane, the inside of a ring
// leaving the curve. Whatever comes to paint behind the plane gives its
// color here.
Item {
    id: plane

    property int radius: Metrics.radiusPanel
    // What is behind the plane.
    property color behind: Theme.color.surfaceWindow

    enabled: false

    Repeater {
        model: 2

        Item {
            required property int index

            x: index === 0 ? 0 : plane.width - width
            width: plane.radius
            height: plane.radius
            clip: true

            // A ring as thick as the radius: its hole is the plane's curve.
            Rectangle {
                x: index === 0 ? -plane.radius : -plane.radius * 2 // derived: the hole's corner on the plane's
                y: -plane.radius
                width: plane.radius * 4 // derived: the hole and the ring around it
                height: width
                radius: plane.radius * 2 // derived: the hole's radius plus the ring
                color: "transparent"
                border.width: plane.radius
                border.color: plane.behind
            }
        }
    }
}
