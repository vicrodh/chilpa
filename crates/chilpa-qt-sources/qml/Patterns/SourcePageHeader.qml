import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Column {
    id: header
    property var info: ({})
    property bool expanded: false
    spacing: Metrics.gap12
    Image { width: Metrics.heroArt; height: width; source: header.info.plugin && header.info.image ? Sources.art(header.info.plugin, header.info.image, Library.remoteArtRev) : header.info.art || ""; visible: source.toString() !== ""; fillMode: Image.PreserveAspectFit; asynchronous: true }
    Text { width: parent.width; visible: !!header.info.title; text: header.info.title || ""; font.pointSize: Type.title; font.weight: Type.bold; color: Theme.color.textPrimary }
    Text { width: parent.width; visible: !!header.info.description; text: header.info.description || ""; textFormat: Text.StyledText; wrapMode: Text.WordWrap; maximumLineCount: header.expanded ? 1000 : 4; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textSecondary }
    PillButton { visible: !!header.info.description; text: header.expanded ? I18n.tr("Show less", I18n.rev) : I18n.tr("Read more", I18n.rev); onClicked: header.expanded = !header.expanded }
    Flow {
        width: parent.width; spacing: Metrics.gap8; visible: !!header.info.ref && !!header.info.plugin
        PillButton { text: I18n.tr("Open", I18n.rev); onClicked: Navigation.goKey("source", header.info.plugin + " native:" + header.info.ref) }
        PillButton { text: header.info.favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev); onClicked: Sources.command("native",header.info.plugin, JSON.stringify({action:"favorite",ref:header.info.ref,on:!header.info.favorite})) }
        PillButton { visible: header.info.kind !== "label"; text: I18n.tr("Pin", I18n.rev); onClicked: Sources.command("native",header.info.plugin, JSON.stringify({action:"pin",item:header.info,on:true})) }
        PillButton { visible: header.info.kind !== "label"; text: I18n.tr("Unpin", I18n.rev); onClicked: Sources.command("native",header.info.plugin, JSON.stringify({action:"pin",item:header.info,on:false})) }
    }
}
