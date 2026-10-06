import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

ListRowSurface {
    id: row
    required property var target
    signal chosen()
    height: Math.max(Metrics.playlistPickerRow, labels.implicitHeight + Metrics.gap16)
    radius: Metrics.radiusControl
    hovered: hover.hovered
    opacity: target.accepts ? 1 : Metrics.disabledOpacity

    Icon {
        id: sourceIcon
        x: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.target.playlist.mixed ? Icons.playlist : Icons.server
        size: Metrics.iconMd
        color: Theme.color.textSecondary
    }
    Column {
        id: labels
        anchors.left: sourceIcon.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2
        Text {
            width: parent.width
            text: row.target.playlist.name
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: (row.target.playlist.mixed ? I18n.tr("Local library", I18n.rev) : row.target.playlist.provider)
                  + " · " + I18n.trCount("{} track", "{} tracks", row.target.playlist.track_count, I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            visible: !row.target.accepts || row.target.exact > 0 || row.target.similar > 0
            text: !row.target.accepts ? I18n.tr("This playlist is read-only or belongs to another source.", I18n.rev)
                : (row.target.exact > 0 ? I18n.trCount("Already contains {} track", "Already contains {} tracks", row.target.exact, I18n.rev) : "")
                  + (row.target.exact > 0 && row.target.similar > 0 ? " · " : "")
                  + (row.target.similar > 0 ? I18n.trCount("Contains {} similar version", "Contains {} similar versions", row.target.similar, I18n.rev) : "")
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: row.target.similar > 0 || !row.target.accepts ? Theme.color.statusWarning : Theme.color.statusPositive
        }
    }
    HoverHandler { id: hover; cursorShape: row.target.accepts ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { enabled: row.target.accepts; onTapped: row.chosen() }
}
