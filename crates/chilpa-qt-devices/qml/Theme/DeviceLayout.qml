pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

QtObject {
    readonly property int selectWidth: Metrics.deviceSelectWidth
    readonly property int rowHeight: Metrics.deviceRowHeight
    readonly property int actionWidth: Metrics.deviceActionWidth
    readonly property int sizeWidth: Metrics.deviceSizeWidth
    readonly property int tableHeader: Metrics.deviceTableHeader
    readonly property int detailHeight: Metrics.deviceDetailHeight
    readonly property int noticeHeight: Metrics.deviceNoticeHeight
    readonly property int galleryHeight: Metrics.deviceGalleryHeight
}
