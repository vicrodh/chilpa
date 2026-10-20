//! I convert the facade rows into the shared track roles.
use crate::{app, format};
use cxx_qt_lib::{QString, QUrl, QVariant};

pub fn header(header: &chilpa_api::GroupHeader, role: i32) -> QVariant {
    match role - 256 {
        0 => QVariant::from(&if header.record { 2i32 } else { 1i32 }),
        3 => QVariant::from(&QString::from(&header.artist)),
        11 => QVariant::from(&QString::from(&header.label)),
        12 => QVariant::from(&QString::from(&header.meta)),
        13 => QVariant::from(&0i64),
        15 => QVariant::from(&if header.art.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(&header.art)) }),
        16 => QVariant::from(&true),
        21 => QVariant::from(&header.work),
        22 => QVariant::from(&QString::from(&header.composer)),
        23 => QVariant::from(&QString::from(&header.composer_target)),
        _ => QVariant::default(),
    }
}

pub fn track(track: &chilpa_api::TrackRow, role: i32, number: i32, playing: bool, selected: bool, group: &str) -> QVariant {
    let text = |s: &str| QVariant::from(&QString::from(s));
    match role - 256 {
        0 => QVariant::from(&0i32),
        1 => QVariant::from(&number),
        2 => text(&track.title),
        3 => text(&track.artist),
        4 => text(&track.album),
        5 => text(&format::clock(track.duration_ms)),
        6 => QVariant::from(&(track.plays as i32)),
        7 => { app::with(|api| api.request_track_quality(track.id, &track.path)); text(&track.quality.short()) },
        8 => QVariant::from(&track.quality.tier.rank()),
        9 => QVariant::from(&playing),
        10 => QVariant::from(&selected),
        13 => QVariant::from(&track.id),
        14 => QVariant::from(&track.album_id),
        15 => {
            let url = if track.artwork_path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(&track.artwork_path)) };
            QVariant::from(&url)
        }
        16 => QVariant::from(&track.available),
        17 => text(&chilpa_api::group_of(group, track)),
        18 => text(&app::with(|api| api.track_source_name(&track.path))),
        19 => text(&app::with(|api| api.track_source_badge(&track.path))),
        20 => text(&track.composer),
        _ => QVariant::default(),
    }
}
