import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The Network sidebar of an artist's page (QBZ): where the artist began
// and ended, its labels, the artists its source holds similar, who played
// in it and what it played in (MusicBrainz), its influences when the source
// knows them, and more artists it may like (Last.fm), each one a row of
// 28 px with a glyph and a name, no portraits. A second tab, Magazine,
// holds the source's stories about the artist. Nothing a source cannot
// resolve is a link.
Rectangle {
    id: panel
    property var document: ({})
    property var network: ({})
    property bool open: true
    signal closed()
    readonly property var header: document.header || ({})
    readonly property bool local: document.local === true
    readonly property string plugin: header.plugin || ""
    function shelf(role) { return (document.shelves || []).find(s => s.role === role) || ({}) }
    readonly property var labels: shelf("labels").items || []
    readonly property var similar: shelf("similar").items || []
    readonly property var influences: shelf("influences").items || []
    readonly property var stories: shelf("stories").items || []
    readonly property var origin: network.origin || null
    readonly property var relationships: network.relationships || ({})
    // Artists the network names that the source does not already, less
    // the ones put aside this session.
    property var dismissed: []
    readonly property var alsoLike: (network.similar || []).filter(name => !dismissed.includes(name) && !similar.some(s => (s.title || "") === name)).slice(0, Metrics.networkPreview)
    property string tab: stories.length > 0 && !network.available ? "magazine" : "network"
    width: open ? Metrics.networkPanel : 0
    Behavior on width { NumberAnimation { duration: Motion.networkSlide; easing.type: Easing.InOutQuad } }
    clip: true
    radius: Metrics.radiusPanel
    color: Theme.ambient ? Theme.color.ambientPanel : Theme.color.surfacePanel
    Edge {}

    function openArtist(item) {
        if (item.ref && panel.plugin) Navigation.goKey("source", panel.plugin + " native:" + item.ref)
        else if (item.local === true) Navigation.openArtist(item.title)
    }
    // A name only: the library's artist when it has one, else the source's
    // best match by name.
    function openByName(name) {
        if (panel.local) Navigation.openArtist(name)
        else if (panel.plugin) Navigation.goKey("source", panel.plugin + " matching-artist-v3:" + name)
    }

    component Heading: Text {
        width: parent.width; textFormat: Text.PlainText; topPadding: Metrics.gap12; bottomPadding: Metrics.gap4
        font.pointSize: Type.label; font.weight: Type.semibold; font.letterSpacing: Type.labelTracking; color: Theme.color.textTertiary
    }
    component Link: Rectangle {
        id: link
        property string glyph: Icons.user
        property string iconFamily: ""
        property string name
        property string tip
        property bool navigable: true
        signal activated()
        signal dismissed()
        property bool dismissible: false
        width: parent.width; height: Metrics.networkRow; radius: Metrics.radiusControl
        color: hover.hovered && navigable ? Theme.color.stateHover : "transparent"
        Icon { id: glyphItem; anchors.left: parent.left; anchors.leftMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; glyph: link.glyph; iconFamily: link.iconFamily; size: Metrics.iconXs; color: Theme.color.textTertiary }
        Text {
            anchors.left: glyphItem.right; anchors.leftMargin: Metrics.gap8; anchors.right: away.visible ? away.left : parent.right; anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter; text: link.name; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.body; color: hover.hovered && link.navigable ? Theme.color.textPrimary : Theme.color.textSecondary
            opacity: link.navigable ? 1 : Metrics.disabledOpacity
        }
        IconButton { id: away; visible: link.dismissible; anchors.right: parent.right; anchors.rightMargin: Metrics.gap4; anchors.verticalCenter: parent.verticalCenter; size: Metrics.button24; glyph: Icons.x; tip: I18n.tr("Not for me", I18n.rev); onClicked: link.dismissed() }
        HoverHandler {
            id: hover; cursorShape: link.navigable ? Qt.PointingHandCursor : Qt.ArrowCursor
            onHoveredChanged: hovered && link.tip !== "" && link.tip !== link.name ? TipState.show(link.tip, link) : TipState.hide(link)
        }
        TapHandler { enabled: link.navigable; onTapped: link.activated() }
    }
    component OriginRow: Item {
        property string key
        property string value
        property bool door: false
        width: parent.width; height: value !== "" ? Metrics.gap22 : 0; visible: value !== ""
        Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: key; font.pointSize: Type.label; font.weight: Type.semibold; font.letterSpacing: Type.labelTracking; color: Theme.color.textTertiary }
        Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; width: Metrics.networkValue; horizontalAlignment: Text.AlignRight; elide: Text.ElideRight; text: value; textFormat: Text.PlainText; font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    // Twelve rows and "Load more", per group of relationships.
    component Group: Column {
        id: group
        property string label
        property var rows: []
        property string glyph: Icons.user
        property bool expanded: false
        width: parent.width; visible: rows.length > 0
        Text { width: parent.width; text: group.label; textFormat: Text.PlainText; topPadding: Metrics.gap6; font.pointSize: Type.label; color: Theme.color.textSecondary }
        Repeater {
            model: group.expanded ? group.rows : group.rows.slice(0, Metrics.networkPreview)
            Link { required property var modelData; glyph: group.glyph; name: modelData.name || ""; tip: modelData.tooltip || ""; onActivated: panel.openByName(modelData.name || "") }
        }
        Text {
            visible: group.rows.length > Metrics.networkPreview; leftPadding: Metrics.gap8; topPadding: Metrics.gap4
            text: group.expanded ? I18n.tr("View less", I18n.rev) : I18n.tr("Load more", I18n.rev); font.pointSize: Type.meta; color: Theme.color.accent
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: group.expanded = !group.expanded }
        }
    }

    Item {
        id: tabs
        width: parent.width; height: Metrics.networkTab
        Row {
            anchors.left: parent.left; anchors.leftMargin: Metrics.networkPad; anchors.bottom: parent.bottom; spacing: Metrics.gap16
            Repeater {
                model: [{ id: "network", title: I18n.tr("Network", I18n.rev) }, { id: "magazine", title: I18n.tr("Magazine", I18n.rev), when: panel.stories.length > 0 || panel.document.loading === true }]
                Item {
                    id: tabItem
                    required property var modelData
                    readonly property bool current: panel.tab === modelData.id
                    visible: modelData.when !== false
                    width: tabLabel.implicitWidth; height: Metrics.networkTab
                    Text { id: tabLabel; anchors.verticalCenter: parent.verticalCenter; text: tabItem.modelData.title; font.pointSize: Type.meta; font.weight: Type.semibold; font.letterSpacing: Type.labelTracking; color: tabItem.current ? Theme.color.textPrimary : Theme.color.textTertiary }
                    Rectangle { visible: tabItem.current; anchors.bottom: parent.bottom; width: parent.width; height: Metrics.gap2; color: Theme.color.accent }
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: panel.tab = tabItem.modelData.id }
                }
            }
        }
        IconButton { anchors.right: parent.right; anchors.rightMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; size: Metrics.button28; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: panel.closed() }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    }
    Flickable {
        anchors.top: tabs.bottom; anchors.bottom: parent.bottom; width: parent.width
        contentWidth: width; contentHeight: body.height + Metrics.gap24; clip: true
        ScrollBar.vertical: ViewportScrollBar {}
        Column {
            id: body
            x: Metrics.networkPad; width: parent.width - Metrics.networkPad * 2 // derived: padding on both sides
            visible: panel.tab === "network"
            // Where it began and ended, when MusicBrainz knows the artist.
            Column {
                width: parent.width; visible: !!panel.origin
                topPadding: Metrics.gap12
                OriginRow { key: panel.origin && panel.origin.person ? I18n.tr("BORN", I18n.rev) : I18n.tr("FOUNDED", I18n.rev); value: panel.origin ? panel.origin.begin || "" : "" }
                OriginRow { key: panel.origin && panel.origin.person ? I18n.tr("BORN IN", I18n.rev) : I18n.tr("FOUNDED IN", I18n.rev); value: panel.origin ? panel.origin.place || "" : "" }
                OriginRow { key: panel.origin && panel.origin.person ? I18n.tr("DIED", I18n.rev) : I18n.tr("DISBANDED", I18n.rev); value: panel.origin ? panel.origin.end || "" : "" }
            }
            Heading { text: I18n.tr("LABELS", I18n.rev) }
            Text { visible: panel.labels.length === 0; leftPadding: Metrics.gap8; text: I18n.tr("No label info", I18n.rev); font.pointSize: Type.body; color: Theme.color.textTertiary }
            Repeater {
                model: panel.labels
                Link { required property var modelData; glyph: Icons.disc; name: modelData.title || ""; navigable: !!modelData.ref && panel.plugin !== ""; onActivated: Navigation.goKey("source", panel.plugin + " native:" + modelData.ref) }
            }
            Heading { visible: panel.similar.length > 0; text: I18n.tr("SIMILAR ARTISTS", I18n.rev) }
            Repeater {
                model: panel.similar
                Link { required property var modelData; name: modelData.title || ""; navigable: !!modelData.ref || modelData.local === true; onActivated: panel.openArtist(modelData) }
            }
            Heading { visible: panel.network.loading === true || panel.relationships.any === true; text: I18n.tr("RELATIONSHIPS", I18n.rev) }
            Text { visible: panel.network.loading === true && panel.relationships.any !== true; leftPadding: Metrics.gap8; text: I18n.tr("Loading…", I18n.rev); font.pointSize: Type.body; color: Theme.color.textTertiary }
            Group { label: I18n.tr("Members & Former", I18n.rev); rows: panel.relationships.members || [] }
            Group { label: I18n.tr("Member Of", I18n.rev); rows: panel.relationships.groups || []; glyph: Icons.music }
            Group { label: I18n.tr("Collaborators", I18n.rev); rows: panel.relationships.collaborators || [] }
            Heading { visible: panel.influences.length > 0; text: I18n.tr("INFLUENCES", I18n.rev) }
            Repeater {
                model: panel.influences
                Link { required property var modelData; name: modelData.title || ""; tip: modelData.role || ""; navigable: !!modelData.ref; onActivated: panel.openArtist(modelData) }
            }
            Heading { visible: panel.alsoLike.length > 0; text: I18n.tr("YOU MAY ALSO LIKE", I18n.rev) }
            Repeater {
                model: panel.alsoLike
                Link { required property string modelData; name: modelData; dismissible: true; navigable: panel.network.resolves === true || panel.local; onActivated: panel.openByName(modelData); onDismissed: panel.dismissed = panel.dismissed.concat([modelData]) }
            }
        }
        Column {
            x: Metrics.networkPad; width: parent.width - Metrics.networkPad * 2 // derived: padding on both sides
            visible: panel.tab === "magazine"; spacing: Metrics.gap8; topPadding: Metrics.gap12
            Text { visible: panel.stories.length === 0; text: I18n.tr("No stories for this artist", I18n.rev); font.pointSize: Type.body; color: Theme.color.textTertiary }
            Repeater {
                model: panel.stories
                Rectangle {
                    id: story
                    required property var modelData
                    width: parent.width; height: storyBody.height + Metrics.gap16; radius: Metrics.radiusCard
                    color: storyHover.hovered ? Theme.color.stateHover : "transparent"
                    Row {
                        id: storyBody; x: Metrics.gap8; y: Metrics.gap8; width: parent.width - Metrics.gap16; spacing: Metrics.gap12
                        ArtTile { width: Metrics.storyArt; height: width; radius: Metrics.radiusArtSmall; source: panel.plugin && story.modelData.image ? Sources.art(panel.plugin, story.modelData.image, Library.remoteArtRev) : "" }
                        Column {
                            width: parent.width - Metrics.storyArt - parent.spacing; spacing: Metrics.gap4
                            Text { width: parent.width; text: story.modelData.title || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
                            Text { width: parent.width; text: story.modelData.author || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.label; color: Theme.color.textTertiary }
                        }
                    }
                    HoverHandler { id: storyHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: if (story.modelData.url) Qt.openUrlExternally(story.modelData.url) }
                }
            }
        }
    }
}
