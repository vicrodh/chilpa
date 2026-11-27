import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

TitledFlow {
    title: "Server playback"
    PillButton { text: "Playback settings…"; onClicked: settings.show(7) }
    ServerPlaybackDialog { id: settings }
}
