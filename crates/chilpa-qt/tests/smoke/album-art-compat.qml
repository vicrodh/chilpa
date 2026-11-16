import QtQuick
import app.chilpa

Item {
    AlbumGridModel { id: records; kind: "all"; columns: 1 }
    Component.onCompleted: {
        const revision = records.rev
        let failures = 0
        try {
            // This is the original plugin's album projection. A failed art
            // call must not leave its caller with an undefined album.
            const album = {id: records.albumId(-1, revision), title: records.title(-1, revision),
                artist: records.subtitle(-1, revision), year: records.year(-1, revision),
                art: String(records.art(-1, revision))}
            if (album.id !== 0 || album.artist !== "" || album.art !== "") failures++
        } catch (error) { console.log("SMOKE FAIL original album projection: " + error); failures++ }
        for (const visible of [false, true]) {
            try {
                if (String(records.art(-1, revision, visible)) !== "") failures++
            } catch (error) { console.log("SMOKE FAIL explicit artwork visibility: " + error); failures++ }
        }
        console.log("SMOKE album-art-compat failures=" + failures)
        Qt.exit(failures ? 1 : 0)
    }
}
