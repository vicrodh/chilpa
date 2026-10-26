import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Row {
    id: pager
    property int page: 0
    property int pages: 1
    property bool busy: false
    property string controlPrefix: "deviceMusic"
    signal pageAsked(int page)
    visible: pages > 1
    height: visible ? implicitHeight : 0
    spacing: Metrics.gap8
    IconButton { objectName: pager.controlPrefix + "PreviousPage"; glyph: Icons.chevronLeft; tip: I18n.tr("Previous page", I18n.rev); enabled: pager.page > 0 && !pager.busy; onClicked: pager.pageAsked(pager.page - 1) }
    Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trArgs("Page {} of {}", [String(pager.page + 1), String(pager.pages)], I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.small }
    IconButton { objectName: pager.controlPrefix + "NextPage"; glyph: Icons.chevronRight; tip: I18n.tr("Next page", I18n.rev); enabled: pager.page + 1 < pager.pages && !pager.busy; onClicked: pager.pageAsked(pager.page + 1) }
}
