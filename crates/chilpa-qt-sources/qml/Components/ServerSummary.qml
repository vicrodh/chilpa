import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The last look before a server is added: the name it goes by here, what it
// is, where it is and what comes in, and what happens next. Once added it
// says so.
Column {
    id: summary
    property string kind: ""
    property var setup: ({})
    property bool done: false
    property var comesIn: []
    readonly property alias label: nameField.text
    spacing: Metrics.gap8

    Component.onCompleted: nameField.text = setup.label || ""
    // The name the server gave itself arrives with each test, and follows
    // the server tested until a name is typed over it.
    property bool named: false
    property string offered: setup.label || ""
    onOfferedChanged: if (!named && offered !== "") nameField.text = offered

    Row {
        visible: summary.done
        width: parent.width
        spacing: Metrics.gap12
        Icon { id: mark; glyph: Icons.circleCheckFilled; filled: true; size: Metrics.iconPlay; color: Theme.color.statusPositive }
        Text {
            objectName: "serverAdded"
            width: parent.width - mark.width - parent.spacing
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.trArgs("{} is in your library now. Its music appears as it is read.", [summary.setup.product || nameField.text], I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.bodyLarge
            color: Theme.color.textPrimary
        }
    }
    Column {
        visible: !summary.done
        width: parent.width
        spacing: Metrics.gap8
        Text { text: I18n.tr("Name in Chilpa", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
        TextField {
            id: nameField
            objectName: "serverLabel"
            width: parent.width
            placeholder: I18n.tr("Server name", I18n.rev)
            onTextEdited: summary.named = true
        }
        Grid {
            width: parent.width
            columns: 2
            columnSpacing: Metrics.gap12
            rowSpacing: Metrics.gap6
            Text { text: I18n.tr("Kind", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
            Row {
                spacing: Metrics.gap6
                SourceIcon { anchors.verticalCenter: parent.verticalCenter; sourceInfo: ({icon: summary.kind === "subsonic" ? "opensubsonic" : summary.kind}) }
                Text {
                    text: summary.kind === "dlna" ? "DLNA / UPnP" : summary.kind === "subsonic" ? "OpenSubsonic" : summary.kind === "plex" ? "Plex" : "Jellyfin"
                    textFormat: Text.PlainText
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
            }
            Text { visible: (summary.setup.product || "") !== ""; text: I18n.tr("What it is", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
            Text { visible: (summary.setup.product || "") !== ""; text: summary.setup.product || ""; textFormat: Text.PlainText; font.pointSize: Type.body; color: Theme.color.textPrimary }
            Text { text: I18n.tr("Address", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
            Text {
                width: summary.width / 2 // derived: the second column
                text: summary.setup.address || ""
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
        }
        Text {
            width: parent.width
            topPadding: Metrics.gap8
            text: summary.kind === "dlna" ? I18n.trCount("{} folder comes in:", "{} folders come in:", summary.comesIn.length, I18n.rev)
                : I18n.trCount("{} library comes in:", "{} libraries come in:", summary.comesIn.length, I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textSecondary
        }
        Repeater {
            model: summary.comesIn
            Text {
                required property string modelData
                width: summary.width
                leftPadding: Metrics.gap8
                text: "• " + modelData
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
        }
        Text {
            width: parent.width
            topPadding: Metrics.gap8
            text: summary.kind === "dlna" ? I18n.tr("Chilpa reads its music now and lists it with yours as it arrives. Nothing on the server changes, and a DLNA / UPnP server has no playlists to bring.", I18n.rev)
                : I18n.tr("Chilpa reads its music now and lists it with yours as it arrives, with its playlists. Nothing on the server changes unless you change one of its playlists.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
    }
}
