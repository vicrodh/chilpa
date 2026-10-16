use super::*;

fn candidate(name: &str, index: u32, valid: bool) -> HardwareVolumeCandidate {
    HardwareVolumeCandidate {
        id: AlsaMixerControlId {
            name: name.to_string(),
            index,
        },
        label: name.to_string(),
        channels: vec!["Front Left".to_string(), "Front Right".to_string()],
        raw_range: Some(HardwareVolumeRange { min: 0, max: 100 }),
        db_range: Some(HardwareVolumeRange { min: -6000, max: 0 }),
        current_values: Vec::new(),
        current_volume: Some(0.5),
        has_playback_switch: true,
        recommended: false,
        writability: HardwareVolumeWritability::Verified,
        rejection_reason: (!valid).then_some(HardwareVolumeRejectionReason::ReadOnly),
    }
}

fn probe(candidates: Vec<HardwareVolumeCandidate>) -> HardwareVolumeProbe {
    HardwareVolumeProbe {
        device_id: "front:CARD=D50,DEV=0".to_string(),
        ctl_name: "hw:CARD=D50".to_string(),
        route_key: Some(StableAlsaRouteKey(
            "alsa-route-v1|card=card:D50|pcm=front|DEV=0".to_string(),
        )),
        candidates,
        ucm_controls: Vec::new(),
        error: None,
    }
}

#[test]
fn unknown_single_candidate_is_selected() {
    let decision =
        decide_hardware_volume(&probe(vec![candidate("D50 III", 0, true)]), &HashMap::new());
    assert!(matches!(
        decision,
        HardwareVolumeDecision::Selected {
            selection: HardwareVolumeSelection {
                source: HardwareVolumeSelectionSource::OnlyCandidate,
                ..
            }
        }
    ));
}

#[test]
fn two_unknown_candidates_need_choice() {
    let decision = decide_hardware_volume(
        &probe(vec![
            candidate("Output A", 0, true),
            candidate("Output B", 0, true),
        ]),
        &HashMap::new(),
    );
    assert!(matches!(
        decision,
        HardwareVolumeDecision::NeedsChoice {
            reason: HardwareVolumeChoiceReason::Ambiguous,
            ..
        }
    ));
}

#[test]
fn same_name_different_indices_remain_distinct() {
    let decision = decide_hardware_volume(
        &probe(vec![candidate("DAC", 0, true), candidate("DAC", 1, true)]),
        &HashMap::new(),
    );
    let HardwareVolumeDecision::NeedsChoice { candidates, .. } = decision else {
        panic!("expected choice");
    };
    assert_eq!(candidates.len(), 2);
    assert_ne!(candidates[0].id, candidates[1].id);
}

#[test]
fn invalid_master_does_not_block_valid_unknown_control() {
    let decision = decide_hardware_volume(
        &probe(vec![
            candidate("Master", 0, false),
            candidate("D50 III", 0, true),
        ]),
        &HashMap::new(),
    );
    let HardwareVolumeDecision::Selected { selection } = decision else {
        panic!("expected selection");
    };
    assert_eq!(selection.control.name, "D50 III");
}

#[test]
fn persisted_exact_match_wins_but_stale_does_not_fall_through() {
    let probe = probe(vec![
        candidate("Master", 0, true),
        candidate("DAC", 0, true),
    ]);
    let route = probe.route_key.clone().unwrap();
    let mut persisted = HashMap::new();
    persisted.insert(
        route.clone(),
        AlsaMixerControlId {
            name: "DAC".into(),
            index: 0,
        },
    );
    let selected = decide_hardware_volume(&probe, &persisted);
    assert!(
        matches!(selected, HardwareVolumeDecision::Selected { selection } if selection.control.name == "DAC")
    );

    persisted.insert(
        route,
        AlsaMixerControlId {
            name: "Gone".into(),
            index: 0,
        },
    );
    assert!(matches!(
        decide_hardware_volume(&probe, &persisted),
        HardwareVolumeDecision::NeedsChoice {
            reason: HardwareVolumeChoiceReason::PersistedSelectionStale,
            ..
        }
    ));
}

#[test]
fn ucm_must_be_unambiguous() {
    let mut probe = probe(vec![
        candidate("Speaker", 0, true),
        candidate("Headphone", 0, true),
    ]);
    probe.ucm_controls = vec![probe.candidates[1].id.clone()];
    assert!(matches!(
        decide_hardware_volume(&probe, &HashMap::new()),
        HardwareVolumeDecision::Selected { selection }
            if selection.source == HardwareVolumeSelectionSource::Ucm
                && selection.control.name == "Headphone"
    ));
    probe.ucm_controls = probe
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect();
    assert!(matches!(
        decide_hardware_volume(&probe, &HashMap::new()),
        HardwareVolumeDecision::NeedsChoice {
            reason: HardwareVolumeChoiceReason::UcmAmbiguous,
            ..
        }
    ));
}

#[test]
fn rejected_ucm_hint_does_not_make_one_valid_ucm_control_ambiguous() {
    let mut probe = probe(vec![
        candidate("Speaker", 0, true),
        candidate("Mic Boost", 0, false),
    ]);
    probe.ucm_controls = probe
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect();

    assert!(matches!(
        decide_hardware_volume(&probe, &HashMap::new()),
        HardwareVolumeDecision::Selected { selection }
            if selection.source == HardwareVolumeSelectionSource::Ucm
                && selection.control.name == "Speaker"
    ));
}

#[test]
fn stable_keys_prefer_serial_then_path_then_symbolic_card_and_include_route() {
    assert_eq!(
        select_stable_card_identity(
            true,
            Some("usb-Fosi_ZH3_REAL123-00".into()),
            Some("pci-usb-port-1".into()),
            "ZH3".into(),
        ),
        StableAlsaCardIdentity::ById("usb-Fosi_ZH3_REAL123-00".into())
    );
    assert_eq!(
        select_stable_card_identity(
            false,
            Some("usb-Fosi_Fosi_Audio_ZH3-00".into()),
            Some("pci-usb-port-1".into()),
            "ZH3".into(),
        ),
        StableAlsaCardIdentity::ByPath("pci-usb-port-1".into())
    );
    assert_eq!(
        select_stable_card_identity(
            false,
            Some("usb-iBasso_Macaron-01".into()),
            None,
            "Macaron".into(),
        ),
        StableAlsaCardIdentity::Card("Macaron".into())
    );

    let by_id = StableAlsaRouteKey::from_parts(
        &StableAlsaCardIdentity::ById("usb-Topping_D50_III_1234-00".into()),
        "front",
        0,
    );
    let by_path = StableAlsaRouteKey::from_parts(
        &StableAlsaCardIdentity::ByPath("pci-0000:00:14.0-usb-0:2:1.0".into()),
        "front",
        0,
    );
    let card =
        StableAlsaRouteKey::from_parts(&StableAlsaCardIdentity::Card("D50".into()), "hdmi", 1);
    assert!(by_id.as_str().contains("card=by-id:"));
    assert!(by_path.as_str().contains("card=by-path:"));
    assert!(card.as_str().contains("card=card:D50"));
    assert!(card.as_str().contains("pcm=hdmi|DEV=1"));
    assert!(!card.as_str().contains("hw:0"));
}

#[test]
fn parser_keeps_pcm_device_in_route_identity() {
    assert_eq!(
        parse_alsa_route("iec958:CARD=PCH,DEV=1"),
        Some(ParsedAlsaRoute {
            card: "PCH".into(),
            pcm: "iec958".into(),
            device: 1,
        })
    );
    assert_eq!(mixer_ctl_name("hw:2,7"), "hw:2");
    assert_eq!(mixer_ctl_name("front:CARD=USB,DEV=0"), "hw:CARD=USB");
}

#[cfg(target_os = "linux")]
#[test]
fn ucm_parser_uses_canonical_alsa_value_identifiers() {
    assert_eq!(
        ucm_parser_identifier("PlaybackVolume"),
        Some("PlaybackVolume")
    );
    assert_eq!(
        ucm_parser_identifier("PlaybackMasterElem"),
        Some("PlaybackMixerId")
    );
    assert_eq!(
        ucm_parser_identifier("PlaybackMixerElem"),
        Some("PlaybackMixerId")
    );
    assert_eq!(ucm_parser_identifier("PlaybackSwitch"), None);
}

#[test]
fn channel_mapping_preserves_balance_and_clamps() {
    let range = HardwareVolumeRange { min: -6000, max: 0 };
    assert_eq!(
        channel_targets(&[-1200, -1800], range, 0.5),
        vec![-3000, -3600]
    );
    assert_eq!(channel_targets(&[-1200, -1800], range, 1.0), vec![0, -600]);
    assert_eq!(
        channel_targets(&[-1200, -1800], range, 0.0),
        vec![-6000, -6000]
    );
}

#[test]
fn recommendation_never_resurrects_rejected_candidate() {
    let mut candidates = vec![candidate("Master", 0, false), candidate("D50 III", 0, true)];
    mark_recommended(&mut candidates);
    assert!(!candidates[0].recommended);
    assert!(candidates[1].recommended);
}

#[test]
fn structural_validation_rejects_unsafe_candidate_shapes_with_exact_reasons() {
    let valid = CandidateFacts {
        name: "D50 III",
        active: true,
        has_playback: true,
        has_capture: false,
        has_common_volume: false,
        raw_range: Some(HardwareVolumeRange { min: 0, max: 100 }),
        playback_channels: 2,
    };
    assert_eq!(structural_rejection(valid), None);
    assert_eq!(
        structural_rejection(CandidateFacts {
            raw_range: Some(HardwareVolumeRange { min: 7, max: 7 }),
            ..valid
        }),
        Some(HardwareVolumeRejectionReason::InvalidRange { min: 7, max: 7 })
    );
    assert_eq!(
        structural_rejection(CandidateFacts {
            playback_channels: 0,
            ..valid
        }),
        Some(HardwareVolumeRejectionReason::NoPlaybackChannels)
    );
    assert_eq!(
        structural_rejection(CandidateFacts {
            has_common_volume: true,
            ..valid
        }),
        Some(HardwareVolumeRejectionReason::CommonPlaybackCaptureVolume)
    );
    assert_eq!(
        structural_rejection(CandidateFacts {
            has_playback: false,
            has_capture: true,
            raw_range: None,
            playback_channels: 0,
            ..valid
        }),
        Some(HardwareVolumeRejectionReason::CaptureOnly)
    );
    assert_eq!(
        structural_rejection(CandidateFacts {
            name: "Mic Boost",
            ..valid
        }),
        Some(HardwareVolumeRejectionReason::UnsafeInputPath)
    );
    assert_eq!(
        writability_rejection(Some(false)),
        Some(HardwareVolumeRejectionReason::ReadOnly)
    );
    assert_eq!(writability_rejection(Some(true)), None);
    assert_eq!(writability_rejection(None), None);
}

#[test]
fn raw_and_db_normalization_use_db_when_complete_and_raw_as_fallback() {
    let raw_range = HardwareVolumeRange { min: 0, max: 100 };
    let db_range = HardwareVolumeRange { min: -6000, max: 0 };
    let db_values = vec![
        HardwareVolumeChannelValue {
            channel: "L".into(),
            raw: 80,
            db_millibels: Some(-3000),
            playback_switch: Some(true),
        },
        HardwareVolumeChannelValue {
            channel: "R".into(),
            raw: 70,
            db_millibels: Some(-3600),
            playback_switch: Some(true),
        },
    ];
    assert!((normalize_level(raw_range, Some(db_range), &db_values) - 0.5).abs() < 0.001);

    let mut broken_db = db_values.clone();
    broken_db[0].db_millibels = None;
    broken_db[1].db_millibels = None;
    assert!((normalize_level(raw_range, Some(db_range), &broken_db) - 0.8).abs() < 0.001);

    let mut partial_db = db_values.clone();
    partial_db[0].db_millibels = None;
    assert!((normalize_level(raw_range, Some(db_range), &partial_db) - 0.8).abs() < 0.001);
}

#[test]
fn mute_unmute_plan_and_partial_channel_switches_are_safe() {
    assert_eq!(
        hardware_volume_write_plan(0.0, true),
        HardwareVolumeWritePlan {
            playback_switch: Some(false),
            write_level: false,
        }
    );
    assert_eq!(
        hardware_volume_write_plan(0.25, true),
        HardwareVolumeWritePlan {
            playback_switch: Some(true),
            write_level: true,
        }
    );
    assert_eq!(
        hardware_volume_write_plan(0.0, false),
        HardwareVolumeWritePlan {
            playback_switch: None,
            write_level: true,
        }
    );

    let range = HardwareVolumeRange { min: 0, max: 100 };
    let mut values = vec![
        HardwareVolumeChannelValue {
            channel: "L".into(),
            raw: 40,
            db_millibels: None,
            playback_switch: Some(false),
        },
        HardwareVolumeChannelValue {
            channel: "R".into(),
            raw: 60,
            db_millibels: None,
            playback_switch: Some(true),
        },
    ];
    assert!((normalize_level(range, None, &values) - 0.6).abs() < 0.001);
    values[1].playback_switch = Some(false);
    assert_eq!(normalize_level(range, None, &values), 0.0);
}

#[test]
fn local_write_feedback_is_suppressed_only_for_matching_recent_echoes() {
    assert!(suppress_local_write_echo(0.42, 10, 0.420_4));
    assert!(!suppress_local_write_echo(0.42, 301, 0.42));
    assert!(!suppress_local_write_echo(0.42, 10, 0.5));
}
