#!/usr/bin/env python3
"""Generate horizontal-speaker PCM and an independent normalized stereo reference."""
from pathlib import Path
import hashlib,json,struct,subprocess

def main():
    root=Path(__file__).resolve().parent
    files=[]
    for channels,mask in [(6,0x60f),(8,0x63f)]:
        count=4096
        data=bytearray()
        for frame in range(count):
            for channel in range(channels):
                if frame<channels*256:
                    value=((frame%127)-63)*200 if frame//256==channel else 0
                elif frame<3072:
                    value=32767 if (frame//64)%2 else -32768
                else:
                    value=((frame*31+channel*173)%20000)-10000
                data.extend(struct.pack('<h',value))
        header=b'RIFF'+struct.pack('<I',60+len(data))+b'WAVEfmt '+struct.pack('<IHHIIHHHHI',40,0xfffe,channels,48000,48000*channels*2,channels*2,16,22,16,mask)+bytes.fromhex('0100000000001000800000aa00389b71')+b'data'+struct.pack('<I',len(data))
        path=root/f'layout-{channels}.wav';path.write_bytes(header+data)
        reference=root/f'layout-{channels}-stereo.f32'
        subprocess.run(['ffmpeg','-nostdin','-y','-v','error','-i',str(path),'-af','aresample=out_chlayout=stereo:rematrix_maxval=1.0','-c:a','pcm_f32le','-f','f32le',str(reference)],check=True)
        assert reference.stat().st_size==count*2*4
        for file in [path,reference]:files.append({'file':file.name,'sha256':hashlib.sha256(file.read_bytes()).hexdigest()})
    (root/'fixtures.json').write_text(json.dumps({'source':'Generated PCM markers, ramps and correlated full-scale controls; no recorded audio.','frames':4096,'rate':48000,'files':files},indent=2)+'\n')

if __name__=='__main__':main()
