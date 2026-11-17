#!/usr/bin/env python3
"""Regenerate finite A/52 markers and reference PCM without audio output.

FFmpeg is a development tool here; playback does not load or invoke it.
The six channels use distinct frequencies, including a 60 Hz LFE.
"""
from pathlib import Path
import hashlib
import json
import math
import struct
import subprocess
import tempfile


def repair_crc(frame):
    def crc(data):
        remainder = 0
        for byte in data:
            for shift in range(7, -1, -1):
                top = remainder & 0x8000
                remainder = ((remainder << 1) & 0xffff) | ((byte >> shift) & 1)
                if top:
                    remainder ^= 0x8005
        return remainder
    size = len(frame)
    boundary = ((size // 2) // 2 + (size // 2) // 8) * 2
    frame[2:4] = b'\0\0'
    target = crc(frame[2:boundary])
    rows = {}
    for bit in range(16):
        candidate = bytearray(boundary - 2)
        candidate[:2] = (1 << bit).to_bytes(2, 'big')
        value, solution = crc(candidate), 1 << bit
        for pivot in range(15, -1, -1):
            if value & (1 << pivot):
                if pivot in rows:
                    value ^= rows[pivot][0]
                    solution ^= rows[pivot][1]
                else:
                    rows[pivot] = (value, solution)
                    break
    solution = 0
    for bit in range(15, -1, -1):
        if target & (1 << bit):
            target ^= rows[bit][0]
            solution ^= rows[bit][1]
    assert target == 0
    frame[2:4] = solution.to_bytes(2, 'big')
    frame[-2:] = b'\0\0'
    frame[-2:] = crc(frame[2:]).to_bytes(2, 'big')
    assert crc(frame[2:boundary]) == 0 and crc(frame[2:]) == 0


def malformed(root):
    container = bytearray((root / 'ac3-six-channel.m4a').read_bytes())
    start = container.index(b'mdat') + 4
    # 448 kbps at 48 kHz: one 1792-byte syncframe.
    original = container[start:start + 1792]
    frame = bytearray(original)
    frame[32:-2] = b'\xff' * (len(frame) - 34)
    repair_crc(frame)
    container[start:start + len(frame)] = frame
    (root / 'crc-valid-invalid-audio.m4a').write_bytes(container)
    for profile in [9, 10]:
        frame = bytearray(original)
        frame[5] = (profile << 3) | (frame[5] & 7)
        repair_crc(frame)
        (root / f'half-rate-{profile}.ac3').write_bytes(frame)


def main():
    root = Path(__file__).resolve().parent
    rate, channels = 48000, 6
    signal = bytearray()
    for frame in range(rate):
        for channel in range(channels):
            frequency = 60 if channel == 3 else 233 + channel * 173
            signal.extend(struct.pack('<f', .24 * math.sin(2 * math.pi * frequency * frame / rate)))
    with tempfile.TemporaryDirectory() as temporary:
        temporary = Path(temporary)
        source = temporary / 'markers.f32'
        source.write_bytes(signal)
        for codec in ['ac3', 'eac3']:
            container = root / f'{codec}-six-channel.m4a'
            raw = temporary / f'markers.{codec}'
            reference = container.with_suffix('.f32')
            commands = [
                ['-f', 'f32le', '-ar', str(rate), '-ac', str(channels), '-channel_layout', '5.1(side)', '-i', str(source), '-threads', '1', '-c:a', codec, '-b:a', '448k', '-dialnorm', '-31', '-f', 'mp4', str(container)],
                ['-i', str(container), '-c:a', 'copy', '-f', codec, str(raw)],
                ['-drc_scale', '1', '-f', codec, '-i', str(raw), '-c:a', 'pcm_f32le', '-f', 'f32le', str(reference)],
            ]
            for command in commands:
                subprocess.run(['ffmpeg', '-nostdin', '-y', '-v', 'error', *command], check=True)
    malformed(root)
    files = [{'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in sorted(root.iterdir()) if path.suffix in ('.m4a', '.f32', '.ac3')]
    (root / 'fixtures.json').write_text(json.dumps({
        'source': 'Generated six-channel markers; no recorded or third-party audio. FFmpeg is only a fixture/reference tool.',
        'rate': rate, 'layout': '5.1(side)', 'bitrate': '448k', 'dialnorm': -31,
        'fixtures': files,
    }, indent=2) + '\n')


if __name__ == '__main__':
    main()
