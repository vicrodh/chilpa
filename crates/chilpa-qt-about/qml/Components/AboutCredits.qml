import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Credits share the About document and are created only while it is open.
Column {
    id: credits
    required property var doc
    signal profileRequested(string url)
    Text {
        text: I18n.tr("Author", I18n.rev)
        font.pointSize: Type.micro
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
        color: Theme.color.textTertiary
    }
    Item { width: 1; height: Metrics.gap12 }
    AboutCredit {
        objectName: "aboutAuthor"
        person: credits.doc.author || ({})
        onActivated: url => credits.profileRequested(url)
    }
    Item { width: 1; height: Metrics.gap24 }
    Text {
        text: I18n.tr("Contributors", I18n.rev)
        font.pointSize: Type.micro
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
        color: Theme.color.textTertiary
    }
    Item { width: 1; height: Metrics.gap12 }
    Flow {
        objectName: "aboutContributorFlow"
        width: parent.width
        spacing: Metrics.gap8
        Repeater {
            model: credits.doc.contributors || []
            AboutCredit {
                required property var modelData
                objectName: "aboutContributor"
                person: modelData
                onActivated: url => credits.profileRequested(url)
            }
        }
    }
}
