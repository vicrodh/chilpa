import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A media server's folders, one level at a time, to choose the ones that
// hold the music: the way down as crumbs to go back up, each folder with
// its circle to choose it and its arrow to open it, and the ones chosen at
// any level, by the way down to each, with a cross to take one out. The
// facade opens each level (`Servers.pick`); nothing is guessed. A folder
// that holds no songs is shown, greyed, with why, and cannot be chosen or
// opened. Settings the server offers (its quality) are below, in its own
// words, each changed only by Apply (`Servers.begin("choose/...")`).
Column {
    id: navigator
    property var setup: ({})
    property var chosen: []
    property var names: ({})
    property bool preview: false
    readonly property bool busy: setup.busy === true
    readonly property var crumbs: setup.crumbs || []
    readonly property string way: crumbs.map(crumb => crumb.name).filter(Boolean).join(" › ")
    readonly property var noMusic: (setup.noMusic || []).map(n => n.id)
    readonly property var musical: (setup.choices || []).filter(choice => noMusic.indexOf(choice.id) < 0)
    signal picked(string id, string name, bool on)
    spacing: Metrics.gap8

    function open(id) { if (!busy && !preview) Servers.pick(id) }

    // Where the list is: the top, then each folder opened.
    Flow {
        width: parent.width
        spacing: Metrics.gap4
        Repeater {
            model: navigator.crumbs
            Row {
                id: crumb
                required property var modelData
                required property int index
                spacing: Metrics.gap4
                Text {
                    visible: crumb.index > 0
                    anchors.verticalCenter: parent.verticalCenter
                    text: "›"
                    font.pointSize: Type.body
                    color: Theme.color.textTertiary
                }
                PillButton {
                    objectName: "serverCrumb_" + crumb.index
                    kind: "ghost"
                    text: crumb.modelData.name || I18n.tr("Top level", I18n.rev)
                    enabled: crumb.index < navigator.crumbs.length - 1 && !navigator.busy && !navigator.preview
                    onClicked: navigator.open(crumb.modelData.id)
                }
            }
        }
    }

    Repeater {
        model: navigator.setup.choices || []
        Rectangle {
            id: folder
            required property var modelData
            required property int index
            objectName: "serverFolder_" + index
            readonly property bool picked: navigator.chosen.indexOf(modelData.id) >= 0
            readonly property bool music: navigator.noMusic.indexOf(modelData.id) < 0
            width: navigator.width
            height: Math.max(Metrics.menuItemRich, choose.implicitHeight)
            radius: Metrics.radiusControl
            color: picked ? Theme.color.rowSelected : Theme.color.fillField
            CircleCheck {
                id: choose
                objectName: "serverChooseFolder_" + folder.index
                anchors.left: parent.left
                anchors.leftMargin: Metrics.gap8
                anchors.right: openIt.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                checkable: false
                checked: folder.picked
                enabled: folder.music
                text: folder.music ? folder.modelData.name : folder.modelData.name + "\n" + folder.modelData.detail
                onClicked: if (folder.music) navigator.picked(folder.modelData.id, navigator.way === "" ? folder.modelData.name : navigator.way + " › " + folder.modelData.name, !folder.picked)
            }
            PillButton {
                id: openIt
                objectName: "serverOpenFolder_" + folder.index
                anchors.right: parent.right
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                kind: "ghost"
                glyph: Icons.chevronRight
                text: I18n.tr("Open", I18n.rev)
                enabled: folder.music && !navigator.busy && !navigator.preview
                onClicked: navigator.open(folder.modelData.id)
            }
        }
    }
    Text {
        visible: !navigator.busy && (navigator.setup.choices || []).length === 0
        width: parent.width
        text: I18n.tr("There are no folders inside this one. Choose it from the level above, or go back up.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }
    Text {
        objectName: "serverOnlyMusic"
        visible: !navigator.busy && navigator.musical.length === 1 && (navigator.setup.choices || []).length > 1
        width: parent.width
        text: navigator.musical.length === 1 ? I18n.trArgs("Only {} holds songs at this level.", [navigator.musical[0].name], I18n.rev) : ""
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Text {
        visible: navigator.busy
        text: I18n.tr("Opening…", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }

    // The server's own settings, as it words them (ServerQuality).
    ServerQuality { width: parent.width; setup: navigator.setup; busy: navigator.busy; preview: navigator.preview }

    // What comes in, chosen at any level.
    Text {
        width: parent.width
        topPadding: Metrics.gap8
        text: navigator.chosen.length === 0 ? I18n.tr("Nothing chosen yet. Choose at least one folder.", I18n.rev)
            : I18n.trCount("{} folder comes in:", "{} folders come in:", navigator.chosen.length, I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
    Repeater {
        model: navigator.chosen
        Row {
            id: kept
            required property string modelData
            required property int index
            width: navigator.width
            spacing: Metrics.gap6
            IconButton {
                id: drop
                objectName: "serverDropFolder_" + kept.index
                size: Metrics.button24
                glyph: Icons.x
                tip: I18n.tr("Take it out", I18n.rev)
                onClicked: navigator.picked(kept.modelData, "", false)
            }
            Text {
                width: parent.width - drop.width - parent.spacing
                anchors.verticalCenter: parent.verticalCenter
                text: navigator.names[kept.modelData] || I18n.tr("A folder chosen before", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
        }
    }
}
