// Choosing the language the window speaks.
//
// The list comes from the bridge, which gets it from the catalogs that ship,
// so adding a translation is adding a `.po` and nothing here changes.
//
// Choosing does not rebuild the window: the bridge bumps `trRev` and every
// binding that asked for a string re-reads it.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

ComboBox {
    // A code with no name of its own shows as the code. It is ugly, but a
    // language nobody wrote a name for is still a language someone can pick.
    readonly property var names: ({ "en": "English", "es": "Español" })

    model: Chilpa.languages.map(c => names[c] !== undefined ? names[c] : c)
    currentIndex: Math.max(0, Chilpa.languages.indexOf(Chilpa.language))
    onActivated: Chilpa.chooseLanguage(Chilpa.languages[index])
}
