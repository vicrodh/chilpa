//! Vorbis delivery is compared with independently decoded signed PCM words.

#[path = "support/vorbis_http.rs"]
mod http;

use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::{DeviceCaps, Policy, Transform},
    sink::{AudioSink, SinkError, StopSignal},
};
use chilpa_core::{
    playback::play_file,
    player::{Player, PlayerEvent},
    queue_playback::{Controls, SharedQueue, play_queue_with},
};
use chilpa_decode::{Block, Decoded};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error, io::MediaSourceStream, probe::Hint,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../chilpa-decode/tests/fixtures")
        .join(name)
}

// Symphonia's sample-buffer conversion is independent of Chilpa's
// interleaving, packing, queue and output code. This is decoded PCM, not
// the PCM master used to create the lossy test file.
fn reference(path: &Path) -> (StreamFormat, Vec<i32>) {
    reference_with_gapless(path, false)
}

fn reference_with_gapless(path: &Path, gapless: bool) -> (StreamFormat, Vec<i32>) {
    let source = MediaSourceStream::new(
        Box::new(std::fs::File::open(path).unwrap()),
        Default::default(),
    );
    let mut format = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            source,
            &symphonia::core::formats::FormatOptions {
                enable_gapless: gapless,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap()
        .format;
    let track = format.default_track().unwrap();
    assert_eq!(
        track.codec_params.codec,
        symphonia::core::codecs::CODEC_TYPE_VORBIS
    );
    assert!(
        track.codec_params.bits_per_sample.is_none(),
        "Vorbis has no encoded integer depth"
    );
    let stream = StreamFormat::new(
        SampleFormat::S32,
        track.codec_params.sample_rate.unwrap(),
        track.codec_params.channels.unwrap().count() as u16,
    );
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .unwrap();
    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(error) => panic!("reference demux failed: {error}"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::DecodeError(_)) => continue,
            Err(error) => panic!("reference decode failed: {error}"),
        };
        let mut pcm = SampleBuffer::<i32>::new(decoded.capacity() as u64, *decoded.spec());
        pcm.copy_interleaved_ref(decoded);
        samples.extend_from_slice(pcm.samples());
    }
    assert!(!samples.is_empty());
    (stream, samples)
}

struct Keep {
    stream: StreamFormat,
    bytes: Arc<Mutex<Vec<u8>>>,
}
impl AudioSink for Keep {
    fn format(&self) -> StreamFormat {
        self.stream
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let frames = (bytes.len() / self.stream.frame_bytes()).min(37);
        self.bytes
            .lock()
            .unwrap()
            .extend_from_slice(&bytes[..frames * self.stream.frame_bytes()]);
        Ok(frames)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

fn caps(stream: StreamFormat) -> DeviceCaps {
    DeviceCaps {
        sample_rates: vec![stream.sample_rate_hz],
        formats: vec![stream.sample_format],
        channels: vec![stream.channels],
        probed: true,
    }
}

fn signed_words(bytes: &[u8]) -> Vec<i32> {
    bytes
        .chunks_exact(4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn identical(actual: &[i32], expected: &[i32]) {
    assert_eq!(actual.len(), expected.len(), "every sample arrived");
    assert_eq!(
        actual.iter().zip(expected).position(|(a, b)| a != b),
        None,
        "first changed PCM word"
    );
}

#[test]
fn the_whole_vorbis_file_reaches_the_sink_as_the_reference_pcm() {
    for name in ["vorbis-44100-stereo.ogg", "vorbis-48000-mono.ogg"] {
        let path = fixture(name);
        let (stream, expected) = reference(&path);
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let played = play_file(
            &path,
            &caps(stream),
            Policy::BitPerfect,
            &StopSignal::new(),
            |opened| {
                assert_eq!(opened, stream);
                Ok(Keep {
                    stream: opened,
                    bytes: bytes.clone(),
                })
            },
        )
        .unwrap();
        assert!(played.delivery.nothing_was_converted());
        identical(&signed_words(&bytes.lock().unwrap()), &expected);
    }
}

#[test]
fn the_network_decoder_uses_the_same_reference_without_an_extension() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, expected) = reference(&path);
    let input = chilpa_stream::HttpStream::new(
        Box::new(std::io::Cursor::new(std::fs::read(path).unwrap())),
        chilpa_stream::StreamInfo {
            content_type: Some("audio/ogg".into()),
            ..Default::default()
        },
        chilpa_stream::NowPlaying::new(),
    );
    let mut decoded = Decoded::open_stream(input).unwrap();
    assert_eq!(decoded.stream_format(), stream);
    let mut samples = Vec::new();
    while let Some(block) = decoded.next_block().unwrap() {
        let Block::Int { data, bits: 32 } = block else {
            panic!("the network reference changed format")
        };
        assert!(
            data.len() <= 8192 * usize::from(stream.channels),
            "bounded decoder blocks"
        );
        samples.extend(data);
    }
    identical(&samples, &expected);
}

#[test]
fn a_narrow_device_still_plays_and_reports_the_lost_reference_bits() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, expected) = reference(&path);
    for word in [
        SampleFormat::S16,
        SampleFormat::S24Packed,
        SampleFormat::S24In32,
        SampleFormat::F32,
    ] {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let opened = StreamFormat {
            sample_format: word,
            ..stream
        };
        let played = play_file(
            &path,
            &caps(opened),
            Policy::BitPerfect,
            &StopSignal::new(),
            |stream| {
                Ok(Keep {
                    stream,
                    bytes: bytes.clone(),
                })
            },
        )
        .unwrap();
        assert!(!played.delivery.nothing_was_converted());
        assert_eq!(played.delivery.opened.sample_rate_hz, stream.sample_rate_hz);
        if word.is_integer() {
            assert!(played.delivery.transforms.contains(&Transform::Narrowed {
                from_bits: 32,
                to_bits: word.valid_bits()
            }));
        } else {
            assert!(played.delivery.transforms.contains(&Transform::Floated));
        }
        assert_eq!(bytes.lock().unwrap().len(), expected.len() * word.bytes());
    }
}

#[test]
fn mono_duplication_keeps_each_reference_word_in_both_channels() {
    let path = fixture("vorbis-48000-mono.ogg");
    let (stream, expected) = reference(&path);
    let stereo = StreamFormat {
        channels: 2,
        ..stream
    };
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let played = play_file(
        &path,
        &caps(stereo),
        Policy::BitPerfect,
        &StopSignal::new(),
        |stream| {
            Ok(Keep {
                stream,
                bytes: bytes.clone(),
            })
        },
    )
    .unwrap();
    assert!(played.delivery.nothing_was_converted());
    assert!(played.delivery.duplicates_mono());
    assert_eq!(
        signed_words(&bytes.lock().unwrap()),
        expected
            .into_iter()
            .flat_map(|s| [s, s])
            .collect::<Vec<_>>()
    );
}

#[test]
fn queue_delivery_keeps_the_reference_and_reports_gain_or_dsp() {
    let path = fixture("vorbis-44100-stereo.ogg");
    // The independent demuxer trims encoder padding itself. The queue's
    // trimmer must produce the same music, including its last frame.
    let (stream, expected) = reference_with_gapless(&path, true);
    assert_eq!(expected.len(), 44_223 * 2);
    for processing in ["bypass", "volume", "dsp"] {
        let controls = Controls::new();
        if processing == "volume" {
            controls.volume.set(0.5);
        }
        if processing == "dsp" {
            controls
                .dsp
                .set(Some(chilpa_audio::dsp::Preset {
                    preamp_db: -6.0,
                    ..Default::default()
                }))
                .unwrap();
        }
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let queue = SharedQueue::new(vec![path.clone()]);
        let mut delivery = None;
        play_queue_with(
            &queue,
            &caps(stream),
            Policy::AllowAnything,
            &controls,
            |stream| {
                Ok(Keep {
                    stream,
                    bytes: bytes.clone(),
                })
            },
            &mut |_, _, said| delivery = Some(said.clone()),
            &mut |_, _| {},
        )
        .unwrap();
        let delivery = delivery.unwrap();
        let samples = signed_words(&bytes.lock().unwrap());
        assert!(!samples.is_empty());
        if processing == "bypass" {
            assert!(delivery.nothing_was_converted());
            identical(&samples, &expected);
        } else {
            assert!(!delivery.nothing_was_converted());
            assert_ne!(samples, expected);
            assert!(delivery.transforms.contains(&if processing == "volume" {
                Transform::VolumeApplied
            } else {
                Transform::DspApplied
            }));
        }
    }
}

fn finished(player: &Player) -> Vec<PlayerEvent> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        events.extend(player.poll());
        if events
            .iter()
            .any(|e| matches!(e, PlayerEvent::Finished { .. } | PlayerEvent::Failed { .. }))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PlayerEvent::Finished { .. })),
        "{events:?}"
    );
    events
}

#[test]
fn the_player_requires_an_exact_output_even_when_the_pcm_is_unchanged() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, expected) = reference_with_gapless(&path, true);
    for (device, exact) in [
        ("file:capture", true),
        ("system:default", false),
        ("unknown", false),
    ] {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let kept = bytes.clone();
        let player = Player::spawn(
            Box::new(move |_| Ok(caps(stream))),
            Box::new(move |_, opened, _| {
                Ok(Box::new(Keep {
                    stream: opened,
                    bytes: kept.clone(),
                }))
            }),
        );
        player.set_exclusive(true);
        player.play(&path, device);
        let events = finished(&player);
        let started: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                PlayerEvent::Started {
                    bit_perfect,
                    opened,
                    bits,
                    ..
                } => {
                    assert_eq!(*bits, None, "no encoded integer depth");
                    Some((*bit_perfect, *opened))
                }
                _ => None,
            })
            .collect();
        assert_eq!(started, [(exact, stream)], "{device}");
        identical(&signed_words(&bytes.lock().unwrap()), &expected);
    }
}

#[test]
fn consecutive_vorbis_tracks_keep_their_final_music_frames_without_reopening() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, music) = reference_with_gapless(&path, true);
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let controls = Controls::new();
    let queue = SharedQueue::new(vec![path.clone(), path]);
    let mut opens = 0;
    let mut starts = Vec::new();
    play_queue_with(
        &queue,
        &caps(stream),
        Policy::BitPerfect,
        &controls,
        |opened| {
            opens += 1;
            Ok(Keep {
                stream: opened,
                bytes: bytes.clone(),
            })
        },
        &mut |i, _, delivery| starts.push((i, delivery.nothing_was_converted())),
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(opens, 1);
    assert_eq!(starts, [(0, true), (1, true)]);
    let expected: Vec<_> = music.iter().chain(&music).copied().collect();
    identical(&signed_words(&bytes.lock().unwrap()), &expected);
}

#[test]
fn native_remote_assets_preserve_the_reference_without_a_file_extension() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, music) = reference_with_gapless(&path, true);
    let server = http::Server::new(std::fs::read(path).unwrap());
    chilpa_core::remote::install(Arc::new(http::Opener(server.url.clone())));
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let kept = bytes.clone();
    let player = Player::spawn(
        Box::new(move |_| Ok(caps(stream))),
        Box::new(move |_, stream, _| {
            Ok(Box::new(Keep {
                stream,
                bytes: kept.clone(),
            }))
        }),
    );
    let entries: Vec<_> = ["one", "two"]
        .map(|id| {
            PathBuf::from(
                chilpa_models::RemoteEntry::new(
                    chilpa_models::SourceId::new("example-source", "test"),
                    id,
                )
                .to_entry(),
            )
        })
        .into();
    player.set_exclusive(true);
    player.play_all(&entries, "file:capture");
    let events = finished(&player);
    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            PlayerEvent::Started {
                bit_perfect,
                total_frames,
                bits,
                ..
            } => {
                assert_eq!(*bits, None, "the provider source stays lossy");
                Some((*bit_perfect, *total_frames))
            }
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 2);
    assert!(
        starts
            .iter()
            .all(|(exact, frames)| *exact && frames.is_some_and(|n| n >= 44_223)),
        "{starts:?}"
    );
    for entry in &entries {
        assert_eq!(
            chilpa_core::remote::opened_codec(entry.to_str().unwrap()),
            Some("vorbis")
        );
    }
    let expected: Vec<_> = music.iter().chain(&music).copied().collect();
    identical(&signed_words(&bytes.lock().unwrap()), &expected);
    assert!(server.requests.load(std::sync::atomic::Ordering::Relaxed) >= 2);
}

#[test]
fn seeking_keeps_the_reference_format_and_exact_music_position() {
    let path = fixture("vorbis-44100-stereo.ogg");
    let (stream, expected) = reference_with_gapless(&path, true);
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let kept = bytes.clone();
    let player = Player::spawn(
        Box::new(move |_| Ok(caps(stream))),
        Box::new(move |_, stream, _| {
            Ok(Box::new(Keep {
                stream,
                bytes: kept.clone(),
            }))
        }),
    );
    for millis in [0, 1, 250, 501, 999] {
        bytes.lock().unwrap().clear();
        player.play_all_from(
            &[path.clone()],
            "file:capture",
            Some(Duration::from_millis(millis)),
        );
        let _ = finished(&player);
        let frame = ((millis * 44_100 + 500) / 1000) as usize;
        identical(
            &signed_words(&bytes.lock().unwrap()),
            &expected[frame * 2..],
        );
    }
}
