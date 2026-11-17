use super::*;
use crate::eac3::ecpl::{self, EcplBlock, EcplChannelParams, EcplCoords, EcplStrategy};
use oxideav_core::bits::BitWriter;

const FIRST_DEFERRED: usize = 1;
const CHANNELS: usize = 3;

#[derive(Clone, Copy)]
enum Changes {
    Rematrix,
    SpectralExtension,
    StandardCoupling,
}

fn stereo_lfe_bsi() -> Ac3Bsi {
    let mut writer = BitWriter::new();
    for (value, bits) in [(8, 5), (0, 3), (2, 3), (0, 2), (1, 1), (31, 5), (0, 8)] {
        writer.write_u32(value, bits);
    }
    let bsi = crate::bsi::parse(&writer.into_bytes()).unwrap();
    assert_eq!(bsi.nfchans, 2);
    assert!(bsi.lfeon);
    bsi
}

fn spectrum(block: usize, channel: usize, bin: usize) -> f32 {
    let value = ((block * 17 + channel * 11 + bin * 7) % 29) as i32 - 14;
    value as f32 / 512.0
}

fn ecpl_input(block: usize, changes: Changes) -> Option<EcplBlock> {
    if block == 0 || matches!(changes, Changes::StandardCoupling) && matches!(block, 2 | 3 | 5) {
        return None;
    }
    let end_subbnd = if matches!(changes, Changes::SpectralExtension) && matches!(block, 1 | 2) {
        6
    } else {
        7
    };
    let strategy = EcplStrategy {
        ecplbegf: 2,
        begin_subbnd: 4,
        end_subbnd,
        bndstrc: [false; ecpl::N_ECPL_SUBBND],
        necplbnd: end_subbnd - 4,
    };
    let bands = strategy.necplbnd;
    let mut mant = [0.0; N_COEFFS];
    for bin in strategy.begin_bin()..strategy.end_bin() {
        mant[bin] = spectrum(block, MAX_FBW, bin);
    }
    Some(EcplBlock {
        mant,
        strategy,
        coords: EcplCoords {
            angleintrp: block % 2 == 0,
            channels: (0..2)
                .map(|ch| EcplChannelParams {
                    param1e: true,
                    param2e: ch == 1,
                    amp: vec![3 + ch as u8; bands],
                    angle: vec![3 + block as u8; bands],
                    chaos: vec![2; bands],
                    trans: ch == 1 && block % 2 == 1,
                })
                .collect(),
        },
        chincpl: [true, true, false, false, false],
    })
}

// Parsed block inputs only. Delay lines and all generator states survive each call.
fn prepare_block(state: &mut Ac3State, block: usize, changes: Changes) {
    let enhanced = ecpl_input(block, changes);
    let standard = matches!(changes, Changes::StandardCoupling) && matches!(block, 2 | 5);
    let spx = matches!(changes, Changes::SpectralExtension) && matches!(block, 1 | 2 | 4);
    state.blkidx = block;
    state.cpl_in_use = enhanced.is_some() || standard;
    state.cpl_begf = if standard && block == 5 { 1 } else { 0 };
    state.cpl_begf_mant = 37 + 12 * state.cpl_begf as usize;
    state.cpl_nsubbnd = if block == 5 { 2 } else { 3 };
    state.cpl_endf_mant = state.cpl_begf_mant + state.cpl_nsubbnd * 12;
    state.cpl_bndstrc = [false; 18];
    state.cpl_bndstrc[1] = block == 2;
    state.cpl_coord = std::array::from_fn(|ch| {
        std::array::from_fn(|band| (1 + ch + band + block) as f32 / 32.0)
    });
    state.cpl_phsflg = std::array::from_fn(|band| (band + block) % 2 == 0);
    state.cpl_bap0 = std::array::from_fn(|bin| standard && (bin + block) % 3 == 0);
    if let Some(input) = &enhanced {
        state.cpl_begf_mant = input.strategy.begin_bin();
        state.cpl_endf_mant = input.strategy.end_bin();
    }
    state.rematflg = if matches!(changes, Changes::Rematrix) {
        [block % 2 == 1, block == 2 || block == 4, false, false]
    } else {
        [false; 4]
    };
    state.spx_in_use = spx;
    state.spx_strtf = u8::from(block == 4);
    state.spx_begin_subbnd = if block == 4 { 4 } else { 3 };
    state.spx_end_subbnd = if block == 4 { 7 } else { 5 };
    state.spx_nbnds = 2;
    state.spx_bndsztab = [0; 18];
    state.spx_bndsztab[0] = if block == 4 { 24 } else { 12 };
    state.spx_bndsztab[1] = 12;
    state.skip_decouple = enhanced.is_some();

    for ch in 0..crate::audblk::MAX_CHANNELS {
        let channel = &mut state.channels[ch];
        channel.coeffs = [0.0; N_COEFFS];
        channel.exp = [4; N_COEFFS];
        channel.in_coupling = ch < 2 && state.cpl_in_use;
        channel.in_spx = ch < 2 && spx;
        channel.dithflag = ch < 2 && standard;
        channel.blksw = ch == 1 && block == 4;
        channel.dynrng = 0.75 + block as f32 / 16.0;
        channel.spx_coord = [0.03125; 18];
        channel.spx_nblend = [0.3 + ch as f32 / 10.0; 18];
        channel.spx_sblend = [0.7; 18];
        channel.spx_atten_active = ch < 2;
        channel.spx_atten_code = 5;
        channel.end_mant = if ch == MAX_FBW + 1 {
            7
        } else if ch == MAX_FBW {
            state.cpl_endf_mant
        } else if channel.in_coupling {
            state.cpl_begf_mant
        } else {
            85
        };
        let start = if ch == MAX_FBW { state.cpl_begf_mant } else { 0 };
        for bin in start..channel.end_mant {
            channel.coeffs[bin] = spectrum(block, ch, bin);
        }
    }
}

fn emit(state: &Ac3State, block: usize, out: &mut [f32]) {
    let base = block * SAMPLES_PER_BLOCK * CHANNELS;
    for sample in 0..SAMPLES_PER_BLOCK {
        for (slot, ch) in [0, 1, MAX_FBW + 1].into_iter().enumerate() {
            out[base + sample * CHANNELS + slot] = state.channels[ch].coeffs[sample];
        }
    }
}

fn synthesize_reference(
    state: &mut Ac3State,
    si: &SyncInfo,
    bsi: &Ac3Bsi,
    block: usize,
    inputs: &[Option<EcplBlock>],
    zero: &EcplBlock,
) {
    if let Some(curr) = &inputs[block] {
        let prev = block.checked_sub(1).and_then(|i| inputs[i].as_ref()).unwrap_or(zero);
        let next = inputs.get(block + 1).and_then(Option::as_ref).unwrap_or(zero);
        let mut coeffs: Vec<_> = (0..ecpl::ECPL_MAX_FBW)
            .map(|ch| state.channels[ch].coeffs)
            .collect();
        ecpl::synthesize_block(&mut state.ecpl_state, prev, curr, next, &mut coeffs, 512);
        for ch in 0..2 {
            state.channels[ch].coeffs = coeffs[ch];
        }
    }
    audblk::dsp_block(state, si, bsi);
    state.skip_decouple = false;
}

fn assert_samples_equal(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(actual.len(), expected.len());
    assert!(expected.iter().any(|value| value.abs() > 1e-7), "{context}: silent fixture");
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        assert!(actual.is_finite() && expected.is_finite(), "{context}: non-finite sample {index}");
        assert!(
            (actual - expected).abs() <= 1e-6,
            "{context}: sample {index}: deferred {actual}, immediate {expected}"
        );
    }
}

fn check_deferred_context(changes: Changes) {
    let si = SyncInfo { crc1: 0, fscod: 0, frmsizecod: 8, sample_rate: 48000, frame_length: 256 };
    let bsi = stereo_lfe_bsi();
    let inputs: Vec<_> = (0..BLOCKS_PER_FRAME).map(|block| ecpl_input(block, changes)).collect();
    let mut zero = inputs[1].as_ref().unwrap().clone();
    zero.mant = [0.0; N_COEFFS];
    let mut expected = Ac3State::new();
    let mut actual = Ac3State::new();
    let mut expected_pcm = vec![0.0; BLOCKS_PER_FRAME * SAMPLES_PER_BLOCK * CHANNELS];
    let mut actual_pcm = expected_pcm.clone();
    let mut channels = Vec::new();
    let mut contexts = Vec::new();

    for block in 0..BLOCKS_PER_FRAME {
        prepare_block(&mut expected, block, changes);
        synthesize_reference(&mut expected, &si, &bsi, block, &inputs, &zero);
        emit(&expected, block, &mut expected_pcm);

        prepare_block(&mut actual, block, changes);
        if block < FIRST_DEFERRED {
            audblk::dsp_block(&mut actual, &si, &bsi);
            emit(&actual, block, &mut actual_pcm);
        } else {
            channels.push(actual.channels.clone());
            contexts.push(BlockDspContext::capture(&actual, inputs[block].is_some()));
        }
    }
    expected.ecpl_state.set_prev_frame_last_mant(inputs.last().unwrap().as_ref().map(|b| b.mant));
    run_deferred_ecpl_dsp(
        &mut actual, &si, &bsi, FIRST_DEFERRED, &inputs, &channels, &contexts,
        2, CHANNELS, true, &mut actual_pcm, None,
    );
    assert_samples_equal(&actual_pcm, &expected_pcm, "frame replay");
    for ch in [0, 1, MAX_FBW + 1] {
        assert_samples_equal(&actual.channels[ch].delay, &expected.channels[ch].delay, "delay");
        assert_samples_equal(&actual.channels[ch].coeffs, &expected.channels[ch].coeffs, "last block");
    }
    assert_eq!(actual.spx_noise_lfsr, expected.spx_noise_lfsr);
    assert_eq!(actual.dither_lfsr_state, expected.dither_lfsr_state);
    assert_eq!(actual.ecpl_state.prev_frame_last_mant(), expected.ecpl_state.prev_frame_last_mant());
    assert_eq!(actual.blkidx, BLOCKS_PER_FRAME - 1);
    assert!(!actual.skip_decouple);

    // The next transient ECPL block observes the live, private ECPL random state.
    // A snapshot that rewound that state would disagree here or in the frame above.
    let continuation = vec![None, ecpl_input(1, Changes::Rematrix), None];
    for state in [&mut actual, &mut expected] {
        prepare_block(state, 1, Changes::Rematrix);
        synthesize_reference(state, &si, &bsi, 1, &continuation, &zero);
    }
    for ch in [0, 1, MAX_FBW + 1] {
        assert_samples_equal(&actual.channels[ch].coeffs, &expected.channels[ch].coeffs, "continuation");
        assert_samples_equal(&actual.channels[ch].delay, &expected.channels[ch].delay, "continuation delay");
    }
}

#[test]
fn deferred_ecpl_keeps_each_blocks_rematrix_flags() {
    check_deferred_context(Changes::Rematrix);
}

#[test]
fn deferred_ecpl_keeps_each_blocks_spectral_extension_context() {
    check_deferred_context(Changes::SpectralExtension);
}

#[test]
fn deferred_ecpl_keeps_standard_coupling_geometry_coordinates_and_dither() {
    check_deferred_context(Changes::StandardCoupling);
}
