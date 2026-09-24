// The card for what is playing: cover, title, format, position and the
// bit-perfect verdict.
//
// The verdict is said as it is. If the output was opened with another
// format, it is marked "converted" in red: a player that says bit-perfect
// without being it is no use at all.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"
    implicitHeight: compact ? 112 : 152

    // In a bottom bar the card does not fit whole: the cover shrinks and
    // the clock stops shouting.
    property bool compact: false

    // mm:ss, which is how a duration is read.
    function clock(seconds) {
        const m = Math.floor(seconds / 60)
        const s = Math.floor(seconds % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: root.compact ? 10 : 16
        spacing: 14

        Rectangle {
            Layout.preferredWidth: root.compact ? 64 : 120
            Layout.preferredHeight: root.compact ? 64 : 120
            radius: 8
            color: "#f0eae4"
            visible: Chilpa.playing || Chilpa.artwork !== ""
            Image {
                anchors.fill: parent
                source: Chilpa.artwork
                fillMode: Image.PreserveAspectCrop
                visible: Chilpa.artwork !== ""
                asynchronous: true
            }
            Label {
                anchors.centerIn: parent
                text: Chilpa.translate("no cover", Chilpa.trRev)
                color: "#7a411d1f"
                font.pixelSize: 11
                visible: Chilpa.artwork === "" && !root.compact
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: root.compact ? 2 : 6

            Label {
                Layout.fillWidth: true
                text: Chilpa.title === "" ? Chilpa.translate("Nothing playing", Chilpa.trRev) : Chilpa.title
                elide: Text.ElideRight
                font.pixelSize: 18
                font.weight: Font.DemiBold
                color: "#411d1f"
            }
            Label {
                Layout.fillWidth: true
                visible: Chilpa.artist !== "" || Chilpa.album !== ""
                text: [Chilpa.artist, Chilpa.album].filter(t => t !== "").join(" · ")
                elide: Text.ElideRight
                font.pixelSize: 14
                color: "#411d1f"
            }
            Label {
                // The real format, and what the analysis found for the
                // track once it has been analysed.
                text: [Chilpa.format, Chilpa.analysis].filter(t => t !== "").join("  ·  ")
                color: "#7a411d1f"
                visible: !root.compact
            }
            Label {
                text: Chilpa.totalSeconds > 0
                    ? clock(Chilpa.seconds) + " / " + clock(Chilpa.totalSeconds)
                    : clock(Chilpa.seconds)
                font.pixelSize: root.compact ? 13 : 26
                color: "#411d1f"
            }

            // The bar also serves for seeking: it is dragged or clicked at any
            // point.
            Slider {
                Layout.fillWidth: true
                visible: Chilpa.totalSeconds > 0
                from: 0
                to: Chilpa.totalSeconds
                value: Chilpa.seconds
                onMoved: Chilpa.seek(value)
            }
            Label {
                text: Chilpa.bitPerfect ? "bit-perfect" : (Chilpa.playing ? Chilpa.translate("converted", Chilpa.trRev) : "")
                color: Chilpa.bitPerfect ? "#2e7d32" : "#d7272b"
            }
            Label {
                Layout.fillWidth: true
                text: Chilpa.error
                wrapMode: Text.Wrap
                color: "#d7272b"
                visible: Chilpa.error !== ""
            }

            Item { Layout.fillHeight: true }
        }
    }
}
