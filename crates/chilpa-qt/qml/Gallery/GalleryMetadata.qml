import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"
import "../Patterns"

// The metadata editor's pieces, fed a view shaped like the facade's, with
// a field in each state: changed, mixed, laid over by a sidecar, invalid.
GallerySection {
    id: section
    title: "Metadata editor"

    function field(name, value, extra) {
        return Object.assign({ name: name, value: value, mixed: false, dirty: false, overridden: false, editable: true, invalid: "" }, extra || {})
    }
    readonly property var sample: ({
        state: "ready", album_id: 1, focus: 0, dirty: true, error: "", notice: "12 of 12 tracks match this release",
        album: [field("title", "Nevermind", { dirty: true }), field("album_artist", "Nirvana"), field("compilation", "false"),
                field("date", "1991-09", { invalid: "a date is a year, a year and a month, or a full day: 1984, 1984-09, 1984-09-03" }),
                field("genres", "", { mixed: true }), field("catalog_number", "DGC-24425", { overridden: true })],
        tracks: [{ id: 1, file: "01 Smells Like Teen Spirit.flac", inside_a_disc: false,
                   fields: [field("title", "Smells Like Teen Spirit", { dirty: true }), field("artist_credit", "Nirvana"),
                            field("track_number", "1"), field("disc_number", "1")] },
                 { id: 2, file: "02 In Bloom.flac", inside_a_disc: false,
                   fields: [field("title", "In Bloom"), field("artist_credit", "Nirvana"), field("track_number", "x", { invalid: "a number from 1 to 9999, or nothing" }), field("disc_number", "1")] }],
        cover: { path: "", waiting: true, width: 1400, height: 1400, from: "musicbrainz" },
        save: { mode: "files", files_allowed: true, files_refused: "", id3: "2.4", secondary_layers: false, done: 0, total: 0 },
        layers: { files: 12, canonical: [{ name: "Vorbis comments", files: 12 }], present: [{ name: "Vorbis comments", files: 12 }, { name: "ID3v2", files: 2 }], conflicting: 2 },
        lookup: { provider: "musicbrainz", searching: false, loading: false, matched: 12,
                  hits: [{ title: "Nevermind", artist: "Nirvana", date: "1991-09-24", country: "US", label: "DGC", catalog_number: "DGC-24425", format: "CD", tracks: 12 },
                         { title: "Nevermind", artist: "Nirvana", date: "2011", country: "XE", label: "Geffen", catalog_number: "", format: "4xCD", tracks: 70 }],
                  release: { title: "Nevermind", date: "1991-09-24", url: "" } },
        artwork: { provider: "musicbrainz", searching: false, loading: false,
                   candidates: [{ index: 0, preview: "", title: "Nevermind", detail: "1991" }, { index: 1, preview: "", title: "Nevermind", detail: "2011" }] }
    })

    MetadataField { width: Metrics.metadataSide; field: section.sample.album[0] }
    MetadataField { width: Metrics.metadataSide; field: section.sample.album[3] }
    MetadataField { width: Metrics.metadataSide; field: section.sample.album[4] }
    MetadataField { width: Metrics.metadataSide; field: section.sample.album[5] }
    MetadataField { width: Metrics.metadataSide; field: section.sample.album[2] }
    MetadataTrackRow { width: section.width; track: section.sample.tracks[0]; chosen: true }
    MetadataTrackRow { width: section.width; track: section.sample.tracks[1]; rowIndex: 1 }
    ReleaseHitRow { width: Metrics.metadataSide; hit: section.sample.lookup.hits[0]; chosen: true }
    ReleaseHitRow { width: Metrics.metadataSide; hit: section.sample.lookup.hits[1]; rowIndex: 1 }
    CoverCandidate { candidate: section.sample.artwork.candidates[0] }
    MetadataHeader { width: section.width; doc: section.sample }
    MetadataFields { width: section.width - Metrics.metadataSide - Metrics.gridGap; doc: section.sample } // derived: the screen's left column
    MetadataArtwork { width: Metrics.metadataSide; doc: section.sample }
    MetadataLookup { width: Metrics.metadataSide; doc: section.sample }
    ConfirmDialog {
        id: confirm
        title: "Write into the audio files?"
        message: "Each file is copied, the copy is written and read back, and only then does it replace the original."
        confirmText: "Write into the files"
    }
    PillButton { text: "Confirm dialog"; onClicked: confirm.open() }
}
