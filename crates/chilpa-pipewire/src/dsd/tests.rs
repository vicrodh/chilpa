use super::*;

unsafe extern "C" {
    fn dsd_choice_contains(kind: u32, values: *const i32, count: u32, wanted: i32) -> bool;
    fn chilpa_pw_dsd_test_format(
        interleave: i32,
        lsb: bool,
        swapped: bool,
        choice: u32,
        rates: *const i32,
        count: u32,
        channels: u32,
        exact_rate: u32,
    ) -> u32;
    fn chilpa_pw_dsd_test_ring() -> bool;
    fn chilpa_pw_dsd_test_cleared_format() -> u32;
    fn dsd_reliable_server(version: *const c_char) -> bool;
    fn dsd_settle_ns(rate: u32, delay: i64, numerator: u32, denominator: u32, chunk: u32) -> u64;
}

#[test]
fn native_dsd_is_offered_only_by_a_server_with_reliable_transport() {
    // Before PipeWire 1.6 a queued chunk can replace one the sink has not
    // read, so native DSD there converts to PCM instead.
    let reliable = |version: &str| {
        let version = CString::new(version).unwrap();
        unsafe { dsd_reliable_server(version.as_ptr()) }
    };
    for version in ["1.6.0", "1.6.9", "1.10.2", "2.0.0"] {
        assert!(reliable(version), "{version} keeps unread handoffs");
    }
    for version in ["1.5.99", "1.4.7", "1.0.5", "0.3.65", "", "next"] {
        assert!(!reliable(version), "{version} has no reliable transport");
    }
    assert!(!unsafe { dsd_reliable_server(std::ptr::null()) });
}

#[test]
fn a_drain_reports_done_once_the_device_has_played_the_last_chunk() {
    let ms = |ns: u64| ns as f64 / 1e6;
    // 1,024 ticks at 48 kHz of device delay, a last chunk of 4,096 frames at
    // 176.4 kHz, and the 2 ms margin.
    let settle = unsafe { dsd_settle_ns(176_400, 1_024, 1, 48_000, 4_096) };
    assert!((ms(settle) - (21.333 + 23.220 + 2.0)).abs() < 0.01, "{} ms", ms(settle));
    // No report from PipeWire still waits for the last chunk.
    assert!((ms(unsafe { dsd_settle_ns(88_200, 0, 0, 0, 2_048) }) - (23.220 + 2.0)).abs() < 0.01);
    // A wrong report cannot hold a stop for long.
    assert_eq!(unsafe { dsd_settle_ns(88_200, i64::MAX / 4, 1, 1, 2_048) }, 500_000_000);
}

#[test]
fn a_suspended_stream_survives_its_cleared_format_and_a_live_one_does_not() {
    // PipeWire suspends a primed or paused stream and clears its format; the
    // stream must wait for it to come back. Losing it while live, or being
    // given another format, still fails.
    assert_eq!(unsafe { chilpa_pw_dsd_test_cleared_format() }, 0b110);
}

fn pod(
    interleave: i32,
    choice: u32,
    rates: &[i32],
    lsb: bool,
    swapped: bool,
    channels: u32,
    exact: u32,
) -> u32 {
    unsafe {
        chilpa_pw_dsd_test_format(
            interleave,
            lsb,
            swapped,
            choice,
            rates.as_ptr(),
            rates.len() as u32,
            channels,
            exact,
        )
    }
}

#[test]
fn endpoint_tuples_do_not_create_a_rate_format_product() {
    let caps = from_caps(Caps {
        serial: 47,
        be_rates: 1,
        le_rates: 8,
    })
    .unwrap();
    assert_eq!(caps.formats, vec![SampleFormat::DsdU32Be]);
    assert_eq!(caps.sample_rates, vec![88_200]);
    assert!(!caps.accepts(&StreamFormat::new(SampleFormat::DsdU32Be, 705_600, 2)));
    let le = from_caps(Caps {
        serial: 47,
        be_rates: 0,
        le_rates: 8,
    })
    .unwrap();
    assert_eq!(le.formats, vec![SampleFormat::DsdU32Le]);
    assert_eq!(le.sample_rates, vec![705_600]);
    assert!(from_caps(Caps::default()).is_none());
    assert!(from_caps(Caps {
        serial: 0,
        be_rates: 15,
        le_rates: 0
    })
    .is_none());
}

#[test]
fn pods_preserve_rates_byte_order_channels_and_positions() {
    for i in 0..4 {
        let rate = 352_800 << i;
        assert_eq!(
            pod(4, 0, &[rate], false, false, 2, (rate / 4) as u32),
            (1 << i) | (1 << 16)
        );
        assert_eq!(
            pod(-4, 0, &[rate], false, false, 2, (rate / 4) as u32),
            (1 << (i + 8)) | (1 << 16)
        );
        assert_eq!(pod(4, 0, &[rate], true, false, 2, (rate / 4) as u32), 0);
        assert_eq!(pod(4, 0, &[rate], false, true, 2, (rate / 4) as u32), 0);
        assert_eq!(pod(4, 0, &[rate], false, false, 6, (rate / 4) as u32), 0);
        assert_eq!(pod(4, 0, &[rate], false, false, 2, 48_000) & (1 << 16), 0);
    }
    for layout in [0, 1, 2, -2] {
        assert_eq!(pod(layout, 0, &[352_800], false, false, 2, 48_000), 0);
    }
}

#[test]
fn enum_range_and_step_choices_are_filtered_without_guessing() {
    assert_eq!(
        pod(4, 3, &[352_800, 352_800, 2_822_400], false, false, 2, 0) & 255,
        9
    );
    assert_eq!(
        pod(4, 1, &[352_800, 352_800, 1_411_200], false, false, 2, 0) & 255,
        7
    );
    assert_eq!(
        pod(
            4,
            2,
            &[352_800, 352_800, 2_822_400, 1_058_400],
            false,
            false,
            2,
            0
        ) & 255,
        5
    );
    assert_eq!(
        pod(4, 2, &[352_800, 352_800, 2_822_400, 0], false, false, 2, 0) & 255,
        0
    );
    assert_eq!(
        pod(4, 1, &[352_800, 2_822_400, 352_800], false, false, 2, 0) & 255,
        0
    );
    let values = [-4, 4];
    assert!(!unsafe { dsd_choice_contains(4, values.as_ptr(), 2, 4) });
    assert!(!unsafe { dsd_choice_contains(0, values.as_ptr(), 0, 4) });
}

#[test]
fn wrapped_partial_ring_writes_preserve_every_channel_byte() {
    assert!(unsafe { chilpa_pw_dsd_test_ring() });
}

#[test]
fn unsupported_requests_never_open_a_server_connection() {
    for (format, exclusive) in [
        (StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2), false),
        (StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 6), true),
        (StreamFormat::new(SampleFormat::DsdU32Be, 48_000, 2), true),
        (StreamFormat::new(SampleFormat::S32, 88_200, 2), true),
    ] {
        assert!(DsdSink::open("invalid", format, exclusive).is_err());
    }
}

/// A real sink suspends a stream that is not playing, which the graph fixture
/// never does: open, a pause long enough to be suspended, resume, a seek and
/// the end. Run with CHILPA_DSD_LIVE_SINK set to the sink's node name.
#[test]
#[ignore = "needs a real PipeWire DSD sink, and plays DSD silence on it"]
fn a_real_sink_plays_after_priming_a_pause_and_a_seek() {
    let id = std::env::var("CHILPA_DSD_LIVE_SINK").expect("CHILPA_DSD_LIVE_SINK names the sink");
    let format = StreamFormat::new(SampleFormat::DsdU32Be, 176_400, 2);
    let mut sink = DsdSink::open(&id, format, true).expect("the sink opens");
    // 0x69 is DSD's idle pattern: silence, never a tone.
    let silence = vec![0x69u8; 8 * 4_410];
    let play = |sink: &mut DsdSink, step: &str| {
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            match sink.write(&silence) {
                Ok(0) => std::thread::sleep(Duration::from_millis(5)),
                Ok(_) | Err(SinkError::Xrun) => {}
                Err(error) => panic!("{step}: {error:?}"),
            }
        }
    };
    play(&mut sink, "first play");
    sink.set_paused(true).expect("pause");
    std::thread::sleep(Duration::from_secs(3));
    sink.set_paused(false).expect("resume");
    play(&mut sink, "after a pause");
    sink.flush().expect("seek");
    std::thread::sleep(Duration::from_millis(500));
    play(&mut sink, "after a seek");
    sink.drain(&|| false).expect("the end");
}
