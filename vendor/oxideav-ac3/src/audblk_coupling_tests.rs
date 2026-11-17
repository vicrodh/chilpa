use super::*;
use oxideav_core::bits::BitWriter;

const COUPLED: [bool; BLOCKS_PER_FRAME] = [true, false, false, true, true, false];
const STRATEGY_PRESENT: [bool; BLOCKS_PER_FRAME] = [true, true, false, true, false, true];
const BANDWIDTH_CODES: [u32; 2] = [0, 4];
const UNCOUPLED_ENDS: [usize; 2] = [73, 85];

fn write_flat_exponents(writer: &mut BitWriter, absolute: u32, groups: usize) {
    writer.write_u32(absolute, 4);
    for _ in 0..groups {
        writer.write_u32(62, 7); // Three zero differences in base 5.
    }
}

fn write_block(writer: &mut BitWriter, block: usize) {
    let coupled = COUPLED[block];
    writer.write_u32(0, 2); // blksw for L and R.
    writer.write_u32(0, 2); // dithflag for L and R.
    writer.write_u32(0, 1); // dynrnge.
    writer.write_u32(u32::from(STRATEGY_PRESENT[block]), 1);
    if STRATEGY_PRESENT[block] {
        writer.write_u32(u32::from(coupled), 1);
        if coupled {
            writer.write_u32(3, 2); // Both channels participate.
            writer.write_u32(0, 1); // phsflginu.
            writer.write_u32(0, 4); // cplbegf, first coefficient 37.
            writer.write_u32(0, 4); // cplendf, end coefficient 73.
            writer.write_u32(0, 2); // Three separate coupling bands.
        }
    }
    if coupled {
        for _ in 0..2 {
            writer.write_u32(1, 1); // cplcoe.
            writer.write_u32(0, 2); // mstrcplco.
            for _ in 0..3 {
                writer.write_u32(0, 4); // cplcoexp.
                writer.write_u32(0, 4); // cplcomant.
            }
        }
    }
    writer.write_u32(1, 1); // rematstr.
    writer.write_u32(0, if coupled { 2 } else { 4 });
    if coupled {
        writer.write_u32(3, 2); // Coupling D45 exponents.
    }
    writer.write_u32(3, 2); // Left D45 exponents.
    writer.write_u32(3, 2); // Right D45 exponents.
    if !coupled {
        for code in BANDWIDTH_CODES {
            writer.write_u32(code, 6);
        }
    }
    if coupled {
        write_flat_exponents(writer, 6, 3); // Absolute coupling exponent 12.
    }
    for channel in 0..2 {
        let groups = if coupled { 3 } else { [6, 7][channel] };
        write_flat_exponents(writer, 12 + channel as u32, groups);
        writer.write_u32(0, 2); // gainrng.
    }
    writer.write_u32(u32::from(block == 0), 1); // baie.
    if block == 0 {
        writer.write_u32(0, 11); // Decays, gains, knee and floor.
    }
    writer.write_u32(1, 1); // snroffste.
    writer.write_u32(0, 6); // Coarse SNR offset.
    if coupled {
        writer.write_u32(0, 7); // Coupling fine SNR offset and fast gain.
    }
    writer.write_u32(0, 14); // L/R fine SNR offsets and fast gains.
    if coupled {
        writer.write_u32(1, 1); // cplleake.
        writer.write_u32(0, 6); // Fast and slow coupling leaks.
    }
    writer.write_u32(u32::from(block == 0), 1); // deltbaie.
    if block == 0 {
        writer.write_u32(2, 2); // No coupling delta allocation.
        writer.write_u32(2, 2); // No left delta allocation.
        writer.write_u32(2, 2); // No right delta allocation.
    }
    writer.write_u32(0, 1); // skiple.
    // Zero SNR offsets give zero baps. Dither is off, so no mantissa bits follow.
}

fn transition_frame() -> (Vec<u8>, [u64; BLOCKS_PER_FRAME]) {
    const FRAME_BYTES: usize = 256; // 48 kHz, 64 kbit/s, frmsizecod 8.
    let mut writer = BitWriter::with_capacity(FRAME_BYTES);
    writer.write_u32(0x0b77, 16);
    writer.write_u32(0, 16); // CRC1 placeholder.
    writer.write_u32(0, 2); // 48 kHz.
    writer.write_u32(8, 6); // 256-byte syncframe.
    writer.write_u32(8, 5); // Base AC-3 bsid.
    writer.write_u32(0, 3); // Complete main service.
    writer.write_u32(2, 3); // Stereo acmod.
    writer.write_u32(0, 2); // dsurmod.
    writer.write_u32(0, 1); // No LFE.
    writer.write_u32(31, 5); // Unity dialogue normalization.
    writer.write_u32(0, 3); // No compression, language or production metadata.
    writer.write_u32(0, 1); // copyrightb.
    writer.write_u32(1, 1); // origbs.
    writer.write_u32(0, 3); // No timecodes or addbsi.

    let mut block_ends = [0; BLOCKS_PER_FRAME];
    for (block, end) in block_ends.iter_mut().enumerate() {
        write_block(&mut writer, block);
        *end = writer.bit_position() - 40; // BitReader starts after syncinfo.
    }
    assert!(writer.bit_position() + 16 < (FRAME_BYTES * 8) as u64);
    while writer.bit_position() < (FRAME_BYTES * 8) as u64 {
        writer.write_u32(0, 1); // Padding, auxdatae and CRC2 placeholder.
    }
    let mut frame = writer.into_bytes();
    let boundary = crate::crc::crc1_boundary_bytes(FRAME_BYTES).unwrap();
    let crc1 = crate::crc::ac3_crc_solve_prefix(&frame[2..boundary]);
    frame[2..4].copy_from_slice(&crc1.to_be_bytes());
    let crc2 = crate::crc::ac3_crc_update(0, &frame[2..]);
    frame[FRAME_BYTES - 2..].copy_from_slice(&crc2.to_be_bytes());
    let status = crate::crc::verify_ac3_syncframe(&frame, FRAME_BYTES);
    assert_eq!(status.crc1_ok, Some(true));
    assert_eq!(status.crc2_ok, Some(true));
    (frame, block_ends)
}

#[test]
fn disabling_coupling_consumes_new_channel_bandwidths_and_exponents() {
    let (frame, block_ends) = transition_frame();
    let sync = crate::syncinfo::parse(&frame).unwrap();
    let bsi = crate::bsi::parse(&frame[5..]).unwrap();
    assert_eq!(sync.frame_length as usize, frame.len());
    assert_eq!(sync.sample_rate, 48_000);
    assert_eq!(bsi.nchans, 2);
    let mut reader = BitReader::new(&frame[5..]);
    reader.skip(bsi.bits_consumed as u32).unwrap();
    let mut state = Ac3State::new();

    for block in 0..BLOCKS_PER_FRAME {
        state.blkidx = block;
        let mut side = AudBlkSideInfo::default();
        parse_audblk_into(&mut state, &sync, &bsi, &mut reader, &mut side)
            .unwrap_or_else(|error| panic!("block {block}: {error}"));
        assert_eq!(reader.bit_position(), block_ends[block], "block {block} cursor");
        assert_eq!(state.cpl_in_use, COUPLED[block]);
        for channel in 0..2 {
            let expected_end = if COUPLED[block] { 37 } else { UNCOUPLED_ENDS[channel] };
            assert_eq!(state.channels[channel].in_coupling, COUPLED[block]);
            assert_eq!(state.channels[channel].end_mant, expected_end);
            assert_eq!(side.chexpstr[channel], 3);
            if !COUPLED[block] {
                assert_eq!(side.chbwcod[channel], BANDWIDTH_CODES[channel] as u8);
            }
            assert!(state.channels[channel].exp[..expected_end]
                .iter().all(|&value| value == 12 + channel as u8));
        }
    }
}

#[test]
fn strict_decode_accepts_complete_frames_with_coupling_transitions() {
    let (frame, _) = transition_frame();
    let sync = crate::syncinfo::parse(&frame).unwrap();
    let bsi = crate::bsi::parse(&frame[5..]).unwrap();
    let mut state = Ac3State::new();
    let mut samples = vec![f32::NAN; BLOCKS_PER_FRAME * SAMPLES_PER_BLOCK * 2];
    for _ in 0..2 {
        decode_frame_strict(&mut state, &sync, &bsi, &frame, &mut samples).unwrap();
        assert!(samples.iter().all(|&sample| sample.is_finite() && sample == 0.0));
        assert!(!state.cpl_in_use);
        for (channel, end) in UNCOUPLED_ENDS.into_iter().enumerate() {
            assert!(!state.channels[channel].in_coupling);
            assert_eq!(state.channels[channel].end_mant, end);
        }
    }
}
