//! WASAPI: the outputs a Windows machine has, in the mode that keeps the
//! samples.
//!
//! Windows offers two ways to reach a device and they are not variations of
//! each other. **Shared** mode hands the samples to the system mixer, which
//! resamples everything to one rate so that a notification can play over the
//! music; **exclusive** mode gives the device to one program and passes the
//! samples through. A player that says bit-perfect and opens in shared mode
//! is saying something untrue, and there is no setting that makes it true.
//!
//! So this stack reports what it can do in exclusive mode, and it measures
//! rather than assumes: `IsFormatSupported` asks the driver whether a format
//! would open, without opening it and without taking the device from anyone.
//! That is the one question in this whole project that Windows answers more
//! honestly than anybody else.
//!
//! **None of this has been run.** It compiles for `x86_64-pc-windows-gnu`,
//! which is checked on every commit, and that is all that is known. The
//! machine it is meant for has not been switched on yet, and nothing here
//! pretends otherwise: `open` refuses rather than half-playing, because an
//! output that opens and delivers nothing looks like it worked.

// Everything here exists only on Windows. The crate is a workspace member,
// so `cargo check --workspace` builds it everywhere, and a platform crate
// that only compiles on its platform is the shape ADR-010 asks for.
#![cfg(target_os = "windows")]

mod read;

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, Knob, OutputBackend};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};

/// Windows' own audio.
pub struct Wasapi;

impl OutputBackend for Wasapi {
    fn kind(&self) -> BackendKind {
        BackendKind::Wasapi
    }

    fn info(&self) -> BackendInfo {
        match read::any_output() {
            Ok(true) => BackendInfo {
                kind: BackendKind::Wasapi,
                present: true,
                missing: String::new(),
                provider: String::new(),
                // Exact, and on this platform that word means exclusive
                // mode. What it costs is in the caveat, because it is not
                // free: the device belongs to Chilpa while a track plays.
                fidelity: Fidelity::Exact,
                caveat: chilpa_i18n::t(
                    "Exact here means the device is Chilpa's alone while a track plays: nothing \
                     else on the machine can use it, not even a notification. Turn bit-perfect \
                     and exclusive mode off to share it and let Windows mix and resample.",
                ),
            },
            Ok(false) => BackendInfo::missing(
                BackendKind::Wasapi,
                chilpa_i18n::t("Windows reports no playback device"),
            ),
            Err(code) => BackendInfo::missing(
                BackendKind::Wasapi,
                chilpa_i18n::t_args("Windows audio would not answer ({})", &[&code.to_string()]),
            ),
        }
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        let default = read::default_output().ok().flatten();
        let mut outputs: Vec<(bool, BackendOutput)> = read::outputs()
            .unwrap_or_default()
            .into_iter()
            .map(|device| {
                let is_default = Some(&device.id) == default.as_ref();
                (
                    is_default,
                    BackendOutput {
                        detail: device.detail(),
                        label: device.name,
                        // Every output here is reached in exclusive mode,
                        // which is what "direct" means on Windows. Whether a
                        // given format opens is a different question, and
                        // `caps` answers it by asking the driver.
                        bit_perfect: true,
                        id: device.id,
                    },
                )
            })
            .collect();
        // What the machine is already using first: somebody opening the list
        // is most likely looking for the one they are hearing.
        outputs.sort_by_key(|(is_default, _)| !is_default);
        outputs.into_iter().map(|(_, o)| o).collect()
    }

    /// What the driver says it would open, in exclusive mode.
    ///
    /// Really measured, and this is the platform where that costs least:
    /// `IsFormatSupported` is a question, not an opening, so it can be asked
    /// while somebody else is playing.
    fn caps(&self, id: &str) -> Result<DeviceCaps, SinkError> {
        read::caps(id).map_err(|code| {
            SinkError::Backend(chilpa_i18n::t_args(
                "Windows would not say what \"{}\" accepts ({})",
                &[id, &code.to_string()],
            ))
        })
    }

    fn volume(&self, id: &str) -> Option<Knob> {
        read::volume(id).ok().flatten()
    }

    fn open(
        &self,
        _id: &str,
        _format: StreamFormat,
        _exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // Declared debt, in ADR-010's sense, and declared on purpose. The
        // render path is the half that cannot be written blind with a
        // straight face: it is a buffer the driver hands out on its own
        // thread on a deadline, and the ways to get it subtly wrong all
        // sound like music with something not quite right. It gets written
        // against a machine that is switched on.
        Err(SinkError::Backend(chilpa_i18n::t(
            "Chilpa can see this machine's outputs but cannot play through them yet",
        )))
    }
}
