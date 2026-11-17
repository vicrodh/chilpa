// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use symphonia_core::{codecs::{CodecParameters, CODEC_TYPE_EAC3},
    errors::{decode_error, Result}, io::ReadBytes};
use super::{Atom, AtomHeader, AtomType};

/// Bounded AC-3/EC-3 sample-entry configuration. The four-byte tag keeps
/// the two bitstream families distinct despite sharing one codec identifier.
#[derive(Debug)]
pub struct A52Atom { header: AtomHeader, extra_data: Box<[u8]> }
impl Atom for A52Atom {
    fn header(&self) -> AtomHeader { self.header }
    fn read<B: ReadBytes>(reader: &mut B, header: AtomHeader) -> Result<Self> {
        let tag: &[u8; 4] = match header.atype {
            AtomType::Ac3DsConfig if header.data_len == 3 => b"dac3",
            AtomType::Eac3DsConfig if (5..=4096).contains(&header.data_len) => b"dec3",
            _ => return decode_error("isomp4 (a52): invalid configuration size"),
        };
        let mut extra_data = vec![0; 4 + header.data_len as usize].into_boxed_slice();
        extra_data[..4].copy_from_slice(tag);
        reader.read_buf_exact(&mut extra_data[4..])?;
        Ok(Self { header, extra_data })
    }
}
impl A52Atom {
    pub fn fill_codec_params(&self, params: &mut CodecParameters) {
        params.for_codec(CODEC_TYPE_EAC3).with_max_frames_per_packet(1536)
            .with_extra_data(self.extra_data.clone());
    }
}
