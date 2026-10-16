import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P

Flickable {
    id: detail
    required property var selection
    required property var document
    property string formatChoice: ""
    property bool busy: false
    signal action(var request)
    signal formatPicked(string value)

    readonly property var formats: selection ? selection.formats || [] : []
    readonly property var tracks: selection && selection.kind === "album" ? selection.tracks || [] : []
    clip: true; contentWidth: width
    contentHeight: body.implicitHeight + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }

    Column {
        id: body
        width: detail.width - Metrics.gap12
        spacing: Metrics.gap16

        Item {
            width: parent.width
            height: Math.max(Metrics.heroArt, information.implicitHeight)
            P.ArtTile {
                id: cover
                width: Metrics.heroArt; height: width
                source: detail.selection ? detail.selection.image || "" : ""
            }
            Column {
                id: information
                anchors.left: cover.right; anchors.leftMargin: Metrics.heroGap
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap8
                Text {
                    width: parent.width
                    text: detail.selection ? detail.selection.title || "" : ""
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: Theme.color.textPrimary; font.pointSize: Type.display; font.weight: Type.bold
                }
                Text { width: parent.width; text: detail.selection ? detail.selection.artist || "" : ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.row }
                Text {
                    width: parent.width
                    text: [detail.selection ? detail.selection.year || detail.selection.album || "" : "",
                        detail.tracks.length ? I18n.trArgs("{} tracks", [detail.tracks.length], I18n.rev) : ""].filter(Boolean).join(" · ")
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: Theme.color.textSecondary; font.pointSize: Type.meta
                }
                Flow {
                    width: parent.width; spacing: Metrics.gap8
                    P.SelectField {
                        options: detail.formats.map(f => f.label)
                        values: detail.formats.map(f => String(f.id))
                        value: detail.formatChoice
                        onPicked: value => detail.formatPicked(value)
                    }
                    P.PillButton {
                        text: detail.selection && detail.selection.kind === "album" ? I18n.tr("Download album", I18n.rev) : I18n.tr("Download track", I18n.rev)
                        kind: "primary"; glyph: Icons.download
                        enabled: !detail.busy && !detail.document.working && detail.document.destination_ready === true
                            && detail.formatChoice !== "" && detail.selection && detail.selection.downloadable !== false
                        onClicked: detail.action({action:"download",format_id:Number(detail.formatChoice)})
                    }
                }
            }
        }
        Text {
            visible: detail.document.destination_ready !== true || detail.selection && detail.selection.downloadable === false
            text: detail.document.destination_ready !== true ? I18n.tr("Choose a download folder before downloading.", I18n.rev)
                : I18n.tr("This purchase is unavailable for download.", I18n.rev)
            color: Theme.color.statusWarning; font.pointSize: Type.body
        }
        Text {
            visible: detail.selection && detail.selection.kind === "album"
            text: I18n.tr("Tracks", I18n.rev)
            color: Theme.color.textPrimary; font.pointSize: Type.section; font.weight: Type.semibold
        }
        Repeater {
            model: detail.tracks
            delegate: PluginPurchaseTrackRow {
                required property var modelData
                required property int index
                width: body.width; entry: modelData; rowIndex: index
                formatId: detail.formatChoice
                canDownload: !detail.busy && !detail.document.working && detail.document.destination_ready === true
                    && detail.formatChoice !== "" && detail.selection.downloadable !== false
                onDownload: detail.action({action:"download",format_id:Number(detail.formatChoice),track_id:modelData.id})
            }
        }
    }
}
