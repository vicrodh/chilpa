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
