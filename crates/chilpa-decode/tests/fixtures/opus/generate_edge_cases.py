#!/usr/bin/env python3
"""Regenerate finite Opus edge cases and reference PCM without audio output.

Requires FFmpeg with the libopus encoder/decoder and a shared libopus.
Each repacketizer input buffer must live until its borrowed frames are emitted.
"""

def main():
    from pathlib import Path
    import ctypes,hashlib,json,struct,subprocess,tempfile
    root=Path(__file__).resolve().parent
    seed=1729
    def signal(n,c):
     nonlocal seed
     out=bytearray()
     for _ in range(n*c):
      seed=(1664525*seed+1013904223)&0xffffffff
      out+=struct.pack('<f',((seed>>8)/16777216-.5)*.05)
     return out
    
    def encode(path,n,c,frame='20'):
     subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','f32le','-ar','48000','-ac',str(c),'-i','pipe:0','-c:a','libopus','-application','lowdelay' if frame=='2.5' else 'audio','-frame_duration',frame,'-b:a','128k','-vbr','off','-y',str(path)],input=signal(n,c),check=True)
    def golden(path):
     out=path.with_suffix('.f32')
     subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-c:a','libopus','-i',str(path),'-f','f32le','-y',str(out)],check=True)
     return out
    
    def packets(path):
     raw=path.read_bytes();at=0;pending=bytearray();out=[]
     while at<len(raw):
      assert raw[at:at+4]==b'OggS'; n=raw[at+26]; seg=raw[at+27:at+27+n]; pos=at+27+n
      for size in seg:
       pending+=raw[pos:pos+size];pos+=size
       if size<255:out.append(bytes(pending));pending.clear()
      at=pos
     assert not pending
     return out
    
    def page(packet,seq,flags,gp):
     seg=bytes([255]*(len(packet)//255)+[len(packet)%255]);h=bytearray(b'OggS\0'+bytes([flags])+struct.pack('<QII',gp,42,seq)+b'\0'*4+bytes([len(seg)])+seg+packet)
     crc=0
     for byte in h:
      crc^=byte<<24
      for _ in range(8):crc=((crc<<1)^0x04c11db7 if crc&0x80000000 else crc<<1)&0xffffffff
     h[22:26]=struct.pack('<I',crc)
     return h
    encode(root/'short-stereo.opus',4800,2);golden(root/'short-stereo.opus')
    with tempfile.TemporaryDirectory() as d:
     p=Path(d)/'base.opus';encode(p,12000,1,'2.5');pk=packets(p)
     lib=ctypes.CDLL('libopus.so.0');lib.opus_repacketizer_create.restype=ctypes.c_void_p
     lib.opus_repacketizer_cat.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_int32]
     lib.opus_repacketizer_out.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_int32]
     lib.opus_repacketizer_destroy.argtypes=[ctypes.c_void_p]
     for count in [32,48]:
      rp=lib.opus_repacketizer_create();assert rp
      try:
       # The repacketizer borrows each source packet until output completes.
       buffers=[ctypes.create_string_buffer(packet) for packet in pk[2:2+count]]
       for data,packet in zip(buffers,pk[2:2+count]):
        assert lib.opus_repacketizer_cat(rp,data,len(packet))==0
       out=ctypes.create_string_buffer(8192);length=lib.opus_repacketizer_out(rp,out,len(out));assert length>0
       packet=out.raw[:length];assert packet[0]&3==3 and packet[1]&63==count
      finally:lib.opus_repacketizer_destroy(rp)
      target=root/f'code3-{count}.opus'
      target.write_bytes(page(pk[0],0,2,0)+page(pk[1],1,0,0)+page(packet,2,4,count*120))
      golden(target)
    files=['short-stereo.opus','short-stereo.f32','code3-32.opus','code3-32.f32','code3-48.opus','code3-48.f32']
    (root/'short-manifest.json').write_text(json.dumps({'source':'deterministic low-level LCG samples; FFmpeg libopus encoder; reference libopus repacketizer for 32/48 frames of 2.5 ms; FFmpeg float decoder reference','sha256':{f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files}},indent=2)+'\n')
    print('Created six offline fixtures and reference samples; no audio output.')
    
    from pathlib import Path
    import hashlib,json,struct,subprocess,tempfile
    root=Path(__file__).resolve().parent
    seed=343
    raw=bytearray()
    for _ in range(72000*6):
     seed=(1664525*seed+1013904223)&0xffffffff
     raw+=struct.pack('<f',((seed>>8)/16777216-.5)*.05)
    target=root/'large-pages.opus'
    subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','f32le','-ar','48000','-ac','6','-i','pipe:0','-c:a','libopus','-b:a','512k','-vbr','off','-frame_duration','20','-y',str(target)],input=raw,check=True)
    encoded=target.read_bytes();at=0;audio=[]
    while at<len(encoded):
     n=encoded[at+26];size=27+n+sum(encoded[at+27:at+27+n]);seq=struct.unpack_from('<I',encoded,at+18)[0]
     if seq>=2:audio.append(size)
     at+=size
    assert len(audio)==2 and sum(audio)>65307 and max(audio)<=65307,audio
    pcm=subprocess.check_output(['ffmpeg','-hide_banner','-loglevel','error','-c:a','libopus','-i',str(target),'-f','f32le','pipe:1'])
    assert len(pcm)==72000*6*4
    ends=root/'large-pages-ends.f32';n=1024*6*4;ends.write_bytes(pcm[:n]+pcm[-n:])
    (root/'large-pages-manifest.json').write_text(json.dumps({'source':'Deterministic LCG float samples encoded by FFmpeg libopus at CBR512kbps, six channels, 1.5seconds; FFmpeg float reference first/last1024frames','frames':72000,'channels':6,'audio_page_bytes':audio,'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [target,ends]}},indent=2)+'\n')
    print('Audio page sizes:',audio)

if __name__ == "__main__":
    main()
