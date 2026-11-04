import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: integration
    required property Item bar
    property bool registerTarget: true
    visible: DisplayMode.active && DisplayMode.provider !== null
    implicitWidth: Metrics.button28
    implicitHeight: Metrics.button28
    IconButton {
        objectName:"displayEject"
        enabled:integration.visible && DisplayMode.provider.canEject
        tip:I18n.tr("Eject",I18n.rev); glyph:Lucide.eject; iconFamily:Lucide.font; size:Metrics.button28
        onClicked:DisplayMode.provider.command("eject")
    }
    DropArea {
        id:target
        objectName:"displayDiscDrop"
        parent:integration.bar; anchors.fill:parent; keys:["app.chilpa.disc"]
        onEntered:drag => { drag.accepted=DisplayMode.active && DisplayMode.provider && DisplayMode.provider.canLoad && DisplayMode.provider.dragAlbum !== null }
        onDropped:drop => { if(DisplayMode.provider && DisplayMode.provider.canLoad && DisplayMode.provider.dragAlbum){DisplayMode.provider.command("drop");drop.acceptProposedAction()} }
    }
    Rectangle { parent:integration.bar; anchors.fill:parent; visible:target.containsDrag; color:"transparent"; radius:Metrics.radiusPlayer; border.color:Theme.color.accent; border.width:Metrics.focusRing }
    Component.onCompleted: if (registerTarget)DisplayMode.standardTarget=bar
    Component.onDestruction:if(DisplayMode.standardTarget === bar)DisplayMode.standardTarget=null
}
