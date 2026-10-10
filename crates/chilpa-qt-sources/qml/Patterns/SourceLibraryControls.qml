import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Row {
    id: controls
    required property string source
    property string entity: "album"
    readonly property string prefix: "ui.source." + source + "."
    property string order: SettingsModel.kept(prefix + "order") || "default"
    property string grouping: SettingsModel.kept(prefix + "group")
    property bool descending: SettingsModel.kept(prefix + "descending") === "true"
    readonly property string sort: order + (descending ? ":desc" : ":asc")
    spacing: Metrics.gap8
    readonly property var orders: {
        const result = [{value: "default", text: I18n.tr("Default", I18n.rev)}, {value: "added", text: I18n.tr("Date added", I18n.rev)}, {value: "title", text: I18n.tr("Alphabetical", I18n.rev)}]
        if (entity === "playlist") result.push({value: "updated", text: I18n.tr("Date updated", I18n.rev)}, {value: "track-count", text: I18n.tr("Track count", I18n.rev)})
        if (entity === "track") result.push({value: "release", text: I18n.tr("Release", I18n.rev)})
        if (entity === "album" || entity === "track") result.push({value: "artist", text: I18n.tr("Artist", I18n.rev)}, {value: "label", text: I18n.tr("Label", I18n.rev)}, {value: "genre", text: I18n.tr("Genre", I18n.rev)}, {value: "release-date", text: I18n.tr("Release date", I18n.rev)})
        if (entity !== "artist") result.push({value: "duration", text: I18n.tr("Duration", I18n.rev)})
        return result
    }
    readonly property var groups: {
        const result = [{value: "", text: I18n.tr("No grouping", I18n.rev)}]
        if (entity === "track") result.push({value: "album", text: I18n.tr("By album", I18n.rev)}, {value: "artist", text: I18n.tr("By artist", I18n.rev)}, {value: "name", text: I18n.tr("By letter", I18n.rev)})
        if (entity === "album" || entity === "artist") result.push({value: "letter", text: I18n.tr("By letter", I18n.rev)})
        if (entity === "album") result.push({value: "artist", text: I18n.tr("By artist", I18n.rev)}, {value: "year", text: I18n.tr("By year", I18n.rev)}, {value: "decade", text: I18n.tr("By decade", I18n.rev)})
        return result
    }
    onPrefixChanged: {
        order = SettingsModel.kept(prefix + "order") || "default"
        grouping = SettingsModel.kept(prefix + "group")
        descending = SettingsModel.kept(prefix + "descending") === "true"
    }
    SortButton {
        objectName: "sourceOrder"
        value: controls.order + (controls.descending ? "-desc" : "")
        options: controls.orders
        onPicked: value => {
            controls.order = baseOf(value)
            controls.descending = value !== controls.order
            SettingsModel.keep(controls.prefix + "order", controls.order)
            SettingsModel.keep(controls.prefix + "descending", String(controls.descending))
        }
    }
    SortButton { visible: controls.groups.length > 1; directed: false; glyph: Icons.stack2; value: controls.grouping; options: controls.groups; onPicked: value => { controls.grouping = value; SettingsModel.keep(controls.prefix + "group", value) } }
}
