import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources

// What asks the window for a page or a fresh look from outside the screens:
// media opened from the disk shows its page, the player's "open media"
// opens the dialog, and a library that changed reads its stars again.
// Ratings turned on or off read the Library again, which filters by them
// or not.
Item {
    Connections { target: OpenMedia; function onOpened() { Navigation.go("openMedia", 0) } }
    Connections { target: Player; function onWindowAsked(what) { if (what === 4) OpenMedia.openRequested() } }
    Connections {
        target: Library
        function onContentRevChanged() { Ratings.refresh() }
        function onRemoteRevChanged() { Ratings.refresh() }
    }
    Connections { target: Prefs; function onRatingsChanged() { Library.applyQuery(Library.queryText) } }
}
