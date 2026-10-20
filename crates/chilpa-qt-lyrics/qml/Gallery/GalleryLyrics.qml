import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Primitives"

TitledFlow {
    title: "Lyrics"
    LyricWords { width: Metrics.rightPanel; nowMs: 1700; endMs: 3000; line: ({ text: "Faded lines", start_ms: 1000, segments: [{ text: "Faded", start_ms: 1000, end_ms: 2000, space_after: true }, { text: "lines", start_ms: 2000, end_ms: 3000, space_after: false }] }) }
    LyricWords { width: Metrics.rightPanel; line: ({ text: "Rivers run under the bridge", start_ms: 0 }) }
    LyricLine { width: Metrics.rightPanel; synced: true; position: 2; current: 2; nowMs: 1700; endMs: 3000; translation: "The same line, translated"; line: ({ text: "Faded lines", start_ms: 1000 }) }
    LyricLine { width: Metrics.rightPanel; synced: true; position: 4; current: 2; size: Type.lyricSmall; uppercase: true; line: ({ text: "Rivers run under the bridge", start_ms: 4000 }) }
    LyricLines { id: lines; width: Metrics.rightPanel; height: Metrics.queueMin; synced: true; current: 1; lines: [{ text: "What went by", start_ms: 0 }, { text: "What is sung", start_ms: 1000 }, { text: "What comes", start_ms: 2000 }, { text: "And after it", start_ms: 3000 }] }
    LyricTracker { view: lines; current: 1; following: false }
    // Not drawn: it reads the translation the lines above show.
    LyricTranslation { lyrics: ({ translation: 0, document: { layers: [{ kind: "translation", lines: [{ line: 0, text: "The same line, translated" }] }] } }) }
}
