"""Independent source/scheduling regressions, not optical accuracy tests.

Synthetic complete attempts have prescribed outputs. Their finite fold is an
independent oracle for arbitrary dispatch partitioning and the4096-attempt
boundary. Actual assembled WGSL is separately compared with production text.
"""
from dataclasses import dataclass
from pathlib import Path
import re
import struct
import unittest


def block(text, opening):
    level=0
    for i in range(opening,len(text)):
        level += (text[i]=='{')-(text[i]=='}')
        if level==0:return i
    raise AssertionError('unterminated block')


def function(text,name):
    signature='fn '+name+'('
    assert text.count(signature)==1
    start=text.index(signature); opening=text.index('{',start)
    return text[start:block(text,opening)+1]


def canonical(text):
    return re.sub(r'\s+','',text.replace('\r\n','\n'))


@dataclass(frozen=True)
class State:
    # Eight float bit patterns plus exact entry bit pattern; never decoded.
    payload:tuple
    entry:int
    count:int
    identity:int
    phase:int

    def roundtrip(self):
        values=struct.unpack('<12I',struct.pack('<12I',*self.payload,self.entry,self.count,self.identity,self.phase))
        return State(values[:8],*values[8:])


def prescribed(attempt):
    # Both rejection classes preserve p/path/u but replace proposed. Accepted
    # attempts write an arbitrary exact word vector. No ray formula is tested.
    if attempt%3==1:return 'reject-error',(0x3f800000+attempt,)
    if attempt%3==2:return 'reject-scene',(0x40000000+attempt,)
    return 'accept',tuple(0x41000000+attempt*16+i for i in range(8))


def sliced(state,quota,stop,transition=prescribed):
    if state.phase:return state,None,[]
    p=list(state.payload); count=state.count; calls=[]; terminal=None
    for _ in range(quota):
        if count==4096:break
        count+=1;calls.append(count)
        if count==stop:
            terminal=('contact',count,tuple(p));break
        kind,value=transition(count)
        if kind.startswith('reject'):p[7]=value[0]
        else:p=list(value)
    if terminal is None and count==4096:terminal=('unresolved',count,tuple(p))
    return State(tuple(p),state.entry,count,state.identity,int(terminal is not None)).roundtrip(),terminal,calls


def oracle(initial,stop):
    # Reduce the prescribed transcript independently of dispatch boundaries.
    last=min(stop,4096) if stop else 4096
    accepted=[i for i in range(1,last+(stop!=last)) if i%3==0]
    payload=list(prescribed(accepted[-1])[1] if accepted else initial.payload)
    prior=last-1 if stop==last else last
    if prior and prior%3:payload[7]=prescribed(prior)[1][0]
    kind='contact' if stop==last else 'unresolved'
    return State(tuple(payload),initial.entry,last,initial.identity,1),(kind,last,tuple(payload))


class WavefrontReview(unittest.TestCase):
    def test_all_attempt_partitions_and_exhaustion_commit(self):
        initial=State(tuple(range(8)),0x42f68000,0,19,0)
        for quota in [1,8,32,128]:
            for stop in [None,1,2,3,7,8,9,31,32,33,127,128,129,4095,4096]:
                state=initial;trace=[];terminal=None
                for _ in range(4096//quota):
                    state,event,calls=sliced(state,quota,stop)
                    trace.extend(calls)
                    if event is not None:self.assertIsNone(terminal);terminal=event
                expected,event=oracle(initial,stop)
                self.assertEqual((state,terminal),(expected,event))
                self.assertEqual(trace,list(range(1,expected.count+1)))
                # Empty later dispatches cannot reemit or mutate a terminal.
                self.assertEqual(sliced(state,quota,stop),(state,None,[]))
        #4095 is accepted;4096 rejects. Exhaustion keeps that accepted state
        # and the final rejection's new proposed word, with no4097 callback.
        exhausted,_=oracle(initial,None)
        self.assertEqual(exhausted.payload[:7],prescribed(4095)[1][:7])
        self.assertEqual(exhausted.payload[7],prescribed(4096)[1][0])
        # Start at4095 with an accepted last attempt to check commit ordering
        # independently of the above synthetic modulo pattern.
        final_payload=tuple(range(100,108))
        before=State(initial.payload,initial.entry,4095,19,0)
        final,event,calls=sliced(before,128,None,lambda attempt:('accept',final_payload))
        self.assertEqual(final.payload,final_payload)
        self.assertEqual(event,('unresolved',4096,final_payload))
        self.assertEqual(calls,[4096])

    def test_bit_preservation_and_initial_terminal_counts(self):
        values=(0,0x80000000,1,0x80000001,0x007fffff,0x00800000,0x7f7fffff,0xff7fffff)
        for phase in [0,1]:
            state=State(values,0x80000000,0,123,phase)
            self.assertEqual(state.roundtrip(),state)
        # Masked/readiness/vacuum exits precede every outer attempt.
        for terminal_kind in ['masked','not-ready','vacuum-miss']:
            state=State(values,values[3],0,123,1)
            self.assertEqual(sliced(state,128,1),(state,None,[]),terminal_kind)

    def test_independent_pixel_schedules_and_publication(self):
        states=[State(tuple(range(8)),100+i,0,i,int(i==0)) for i in range(9)]
        stops=[0,1,8,9,31,128,4096,None,3]
        completed=1;live=8;published=False;emitted=[]
        for round_index in range(4096):
            # Reverse ownership order every other round; no state can depend
            # on another ray or confuse output-record ID with lane position.
            order=range(9) if round_index%2 else reversed(range(9))
            for i in order:
                state,event,_=sliced(states[i],1,stops[i])
                states[i]=state
                if event is not None:live-=1;completed+=1;emitted.append(i)
            now=live==0 and completed==len(states)
            if not now:self.assertTrue(any(not x.phase for x in states))
            published |= now
        self.assertTrue(published)
        self.assertEqual((live,completed),(0,9))
        self.assertEqual(sorted(emitted),list(range(1,9)))
        self.assertEqual([s.identity for s in states],list(range(9)))
        for i in range(1,9):
            expected,_=oracle(State(tuple(range(8)),100+i,0,i,0),stops[i])
            self.assertEqual(states[i],expected)

    def test_actual_assembled_attempt_and_initialization_text(self):
        production=Path('src/desktop/shaders/refraction.wesl').read_text()
        generated=Path('artifacts/optical-wavefront-validation-first.wgsl').read_text()
        trace=function(production,'trace');loop=trace.index('for(var i=0;i<4096;i+=1)')
        opening=trace.index('{',loop);end=block(trace,opening)
        prefix=trace[trace.index('{')+1:loop];body=trace[opening+1:end]
        init=function(generated,'wave_initialize_trace')
        emitted=init[init.index('{')+1:init.rfind('wave_store(')]
        pattern=r'wave_complete\(id,(.*?),state,path,proposed,path,0u\);return;'
        emitted,count=re.subn(pattern,r'return \1;',emitted)
        self.assertEqual(count,3)
        self.assertEqual(canonical(emitted),canonical(prefix))
        shader_slice=function(generated,'wave_slice')
        opening=shader_slice.index('{',shader_slice.index('for(var local='));end=block(shader_slice,opening)
        emitted=shader_slice[opening+1:end]
        self.assertEqual(emitted.count('attempt+=1u;'),1)
        emitted=emitted.replace('attempt+=1u;','')
        pattern=r'wave_complete\(id,(.*?),state,path,proposed,entry,attempt\);return;'
        emitted,count=re.subn(pattern,r'return \1;',emitted)
        self.assertEqual(count,11)
        self.assertEqual(emitted.count('continue;'),2)
        self.assertEqual(canonical(emitted),canonical(body))
        self.assertIn('attempt<4096u',shader_slice)
        self.assertIn('if attempt==4096u{wave_complete(id,trace_result(state.p,state.u,path,2u)',shader_slice)
        store=function(generated,'wave_store')
        self.assertEqual(store.count('bitcast<vec4<u32>>'),2)
        self.assertIn('vec4(bitcast<u32>(entry),attempt,id,phase)',store)
        self.assertNotIn('normalize',store)
        finalize=canonical(function(generated,'wave_finalize'))
        self.assertIn('atomicLoad(&wave_progress[0])==0u&&atomicLoad(&wave_progress[1])==23u',finalize)

    def test_reduction_metadata_boundaries_and_invalid_records(self):
        # Per-block reduction is compared with one flat independent predicate
        # count. Include incomplete and malformed metadata, not only all-done.
        for count in [1,23,255,256,257,511,512,513,2073600]:
            records=[(i%4097,i,1) for i in range(count)]
            def check(rows):
                partials=[]
                for start in range(0,len(rows),256):
                    live=done=bad=maximum=0
                    for identity,(attempt,stored_id,phase) in enumerate(rows[start:start+256],start):
                        maximum=max(maximum,attempt)
                        live+=phase==0;done+=phase==1
                        bad+=stored_id!=identity or attempt>4096 or phase>1
                    partials.append((live,done,maximum,bad))
                reduced=(sum(x[0] for x in partials),sum(x[1] for x in partials),
                         max(x[2] for x in partials),sum(x[3] for x in partials))
                flat=(sum(x[2]==0 for x in rows),sum(x[2]==1 for x in rows),max(x[0] for x in rows),
                      sum(x[0]>4096 or x[1]!=i or x[2] not in [0,1] for i,x in enumerate(rows)))
                self.assertEqual(reduced,flat)
                self.assertLessEqual(reduced[0]+reduced[1],count)
                return reduced[0]==0 and reduced[1]==count and reduced[3]==0
            self.assertTrue(check(records))
            if count>513:continue  # maximum count already establishes sum range
            for bad in [(0,0,0),(4097,0,1),(0,0,2),(0,0xffffffff,1)]:
                damaged=records.copy();damaged[0]=bad
                self.assertFalse(check(damaged))


if __name__=='__main__':unittest.main()
