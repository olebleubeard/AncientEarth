"""Independent scheduling/partition checks, not an ODE or GPU-rounding test."""
from fractions import Fraction as Q
from itertools import product
from pathlib import Path
from dataclasses import dataclass, field
import re
import unittest

from test_optical_normal_intervals import bits,value


def function(source,name):
    start=source.index('fn '+name+'(')
    opening=source.index('{',start);depth=0
    for i in range(opening,len(source)):
        if source[i]=='{':depth+=1
        elif source[i]=='}':
            depth-=1
            if depth==0:return source[opening+1:i]
    raise AssertionError(name)


def source_unpersist(text):
    text=text.replace('ns.height_attempted','height.attempted_height').replace('ns.height_path','height.path')
    return re.sub(r'\bns\.(state|path|proposed|height_proposed|height_retries|owner)\b',r'\1',text)


def expected_decision(end,events):
    """Direct ordered intervals, with no suspension state or phase encoding."""
    n=len(events)
    for i,event in enumerate(events):
        if Q(i,n)>=end:break
        if event!='clear':return event,False
    return 'clear',True


def suspended_decision(end,events,ceiling_clear):
    # A synthetic accepted interval changes state and source owner only when
    # every physical prefix piece is clear. A scheduling round does not itself
    # consume an ODE attempt or confer a successful scene result.
    state=11;owner=3;attempts=41;index=0;phase='certificate';saved=[]
    count=len(events)
    for _ in range(4*count+4):
        saved.append((state,owner,attempts))
        if phase=='certificate':
            while index<count and Q(index,count)<end and index in ceiling_clear:index+=1
            if index==count or Q(index,count)>=end:phase='commit'
            else:phase='consume'
        elif phase=='consume':
            # The original accepted end clips only the physical query; the
            # certificate's interval remains the full aligned dyadic piece.
            interval=(Q(index,count),min(end,Q(index+1,count)))
            assert interval[0]<interval[1]<=end
            event=events[index]
            if event!='clear':return event,state,owner,attempts,saved
            index+=1;phase='certificate'
        else:
            state=12;owner=4
            return 'clear',state,owner,attempts,saved
    raise AssertionError('schedule bound')


@dataclass(frozen=True)
class Trial:
    events: tuple = ('clear',)
    end: Q = Q(1)
    ceiling: frozenset = frozenset()
    first_certificate: bool = True
    later_failure: frozenset = frozenset()
    rejected: bool = False
    whole_clear: bool = False
    exits: bool = False


def direct_transcript(trials, initial_attempt=0):
    """Reduce complete attempt outcomes without a stage/partition state machine."""
    physical=11;owner=3;commits=[]
    for offset,trial in enumerate(trials,1):
        attempt=initial_attempt+offset
        if attempt>4096:break
        if trial.rejected:continue
        event='clear'
        if not trial.whole_clear:
            if not trial.first_certificate:event='uncertain'
            else:
                for index,kind in enumerate(trial.events):
                    if Q(index,len(trial.events))>=trial.end:break
                    if index in trial.ceiling:continue
                    event='uncertain' if index in trial.later_failure else kind
                    if event!='clear':break
        if event=='hit':return ('hit',physical,owner,attempt,commits)
        if event=='uncertain':continue
        physical+=1;owner+=1;commits.append(attempt)
        if trial.exits:return ('sky',physical,owner,attempt,commits)
    if initial_attempt+len(trials)>=4096:
        return ('exhausted',physical,owner,4096,commits)
    return ('pending',physical,owner,initial_attempt+len(trials),commits)


@dataclass
class SplitSchedule:
    trials: tuple
    attempt: int = 0
    physical: int = 11
    owner: int = 3
    phase: int = 0
    pieces: int = 0
    index: int = 0
    cursor: int = 0
    event: str = ''
    terminal: str = 'pending'
    commits: list = field(default_factory=list)
    snapshots: list = field(default_factory=list)
    first_admissions: list = field(default_factory=list)
    current: Trial | None = None

    def publish(self,kind):
        assert self.phase!=4
        self.phase=4;self.terminal=kind

    def commit(self):
        if self.event=='hit':self.publish('hit');return
        if self.event=='clear':
            self.physical+=1;self.owner+=1;self.commits.append(self.attempt)
            if self.current.exits:self.publish('sky');return
        self.phase=0

    def produce(self):
        if self.phase in (1,2,4):return
        if self.phase==3:
            if self.event=='clear':
                next_piece=self.index+1
                # Short circuit is essential when a first certificate failed
                # and the private partition sentinel is still zero.
                if next_piece<self.pieces and Q(next_piece,self.pieces)<self.current.end:
                    self.index=next_piece;self.phase=1;return
            self.commit()
            if self.phase==4:return
        while self.phase==0:
            if self.attempt==4096:self.publish('exhausted');return
            if self.cursor==len(self.trials):return
            self.current=self.trials[self.cursor];self.cursor+=1;self.attempt+=1
            if self.current.rejected:continue
            if self.current.whole_clear:
                self.event='clear';self.commit()
                if self.phase==4:return
                continue
            self.phase=1;self.pieces=0;self.index=0;return

    def certify(self):
        if self.phase!=1:return
        if self.pieces==0:
            self.first_admissions.append((self.attempt,self.index))
            if not self.current.first_certificate:
                self.event='uncertain';self.phase=3;return
            self.pieces=len(self.current.events);self.index=0
        while self.index<self.pieces and Q(self.index,self.pieces)<self.current.end and self.index in self.current.ceiling:
            self.index+=1
        if self.index>=self.pieces or Q(self.index,self.pieces)>=self.current.end:
            self.event='clear';self.phase=3;return
        if self.index in self.current.later_failure:
            self.event='uncertain';self.phase=3;return
        self.phase=2

    def consume(self):
        if self.phase!=2:return
        self.event=self.current.events[self.index];self.phase=3

    def run(self,rounds=40):
        for _ in range(rounds):
            for stage in (self.produce,self.certify,self.consume):
                before=(self.physical,self.owner,self.attempt)
                stage()
                if stage!=self.produce:
                    assert before==(self.physical,self.owner,self.attempt)
                self.snapshots.append((self.phase,self.physical,self.owner,self.attempt,self.pieces,self.index))
        return (self.terminal,self.physical,self.owner,self.attempt,self.commits)


class SelectedContinuationTests(unittest.TestCase):
    def test_exact_partition_and_original_prefix(self):
        for count in [1,2,4,8,16,32,64]:
            inverse=value((254-(bits(float(count))>>23))<<23)
            self.assertEqual(Q(inverse),Q(1,count))
            ranges=[(Q(float(i)*inverse),Q(float(i+1)*inverse)) for i in range(count)]
            self.assertEqual(ranges[0][0],0);self.assertEqual(ranges[-1][1],1)
            self.assertTrue(all(a[1]==b[0] for a,b in zip(ranges,ranges[1:])))
            for end in [Q(1),Q(1,2),Q(3,10),Q(1,1000)]:
                active=[(a,min(b,end)) for a,b in ranges if a<end]
                self.assertEqual(sum(b-a for a,b in active),end)
                self.assertEqual(active[-1][1],end)

    def test_no_source_commit_before_all_chronological_pieces_clear(self):
        cases=0
        for events in product(['clear','hit','uncertain'],repeat=4):
            for end in [Q(1),Q(1,2),Q(3,10)]:
                for clear_set in [set(),{i for i,e in enumerate(events) if e=='clear'}]:
                    expected,committed=expected_decision(end,events)
                    actual,state,owner,attempts,saved=suspended_decision(end,events,clear_set)
                    self.assertEqual(actual,expected)
                    self.assertEqual((state,owner),(12,4) if committed else (11,3))
                    self.assertEqual(attempts,41)
                    self.assertTrue(all(s==(11,3,41) for s in saved))
                    cases+=1
        self.assertEqual(cases,486)

    def test_extracted_attempt_and_commit_preserve_original_source(self):
        original=Path('scripts/optical_normal_scene_trace.wgsl').read_text()
        assembled=Path('artifacts/optical-normal-selected-validation-second-producer-original.wgsl').read_text()
        body=function(original,'nc_trace_scene')
        marker='for(var attempt=1u;attempt<=4096u;attempt+=1u){'
        prefix=body.split(marker,1)[1].split('        let event=nc_scene_query(accepted_curve,tolerance);',1)[0]
        actual=function(assembled,'ns_attempt').split('        ns.attempt=attempt;',1)[1].split('        ns.curve=accepted_curve;',1)[0]
        self.assertEqual(source_unpersist(actual).strip(),prefix.strip())
        suffix=body.split('        let event=nc_scene_query(accepted_curve,tolerance);',1)[1]
        suffix=suffix.rsplit('    }\n    return nc_result',1)[0]
        actual=function(assembled,'ns_commit')
        self.assertIn('let tolerance=ns.tolerance;',actual)
        actual=actual[actual.index('        if event.kind==2u{'):]
        actual=actual.rsplit('    return ns_signal(4u);',1)[0]
        actual=source_unpersist(actual).replace('return ns_signal(4u);','continue;')
        self.assertEqual(actual.strip(),suffix.strip())
        self.assertIn('return nc_result(ns.state,ns.path,2u,4096u,6u);',function(assembled,'ns_attempt'))
        # Every attempt, including rejections, increments before its old body;
        # committing an accepted attempt4096 precedes the exhaustion return.
        self.assertIn('for(var attempt=ns.attempt+1u;attempt<=4096u;attempt+=1u)',assembled)

    def test_live_source_partition_and_stored_prefix_contract(self):
        source=Path('scripts/optical_normal_selected_control.wgsl').read_text()
        partition=function(source,'ns_piece_range')
        self.assertNotIn('/',re.sub(r'//[^\n]*','',partition))
        self.assertIn('bitcast<f32>',partition)
        self.assertIn('query.theta_end=min(ns.curve.theta_end,nc_cartesian_piece.parameter.y)',source)
        state=Path('scripts/optical_normal_selected_state.wgsl').read_text()
        for required in ['ns.curve.theta_end','ns.curve.numerical_error','ns.height_retries',
                         'nc_lower_gap','nc_upper_gap','nc_scene_anchor_residual',
                         'nc_root_count','nh_attempts','nh_accepted','nc_scene_visits']:
            self.assertIn(required,function(state,'ns_store'))
            self.assertIn(required if required not in ['ns.curve.theta_end','ns.curve.numerical_error'] else 'ns.curve=NcSceneCurve',function(state,'ns_load'))

    def test_split_rounds_match_complete_attempt_transcripts(self):
        choices=[Trial(rejected=True),Trial(first_certificate=False),
                 Trial(events=('clear','hit')),Trial(events=('clear','uncertain')),
                 Trial(events=('clear',)*4,ceiling=frozenset({1,2,3})),
                 Trial(events=('clear',)*4,end=Q(3,10)),
                 Trial(events=('clear',)*4,later_failure=frozenset({2})),
                 Trial(whole_clear=True)]
        cases=0
        for history in product(choices,repeat=3):
            trials=history+(Trial(exits=True),)
            schedule=SplitSchedule(trials)
            self.assertEqual(schedule.run(),direct_transcript(trials))
            self.assertTrue(all(index==0 for _,index in schedule.first_admissions))
            self.assertEqual(len({a for a,_ in schedule.first_admissions}),len(schedule.first_admissions))
            cases+=1
        self.assertEqual(cases,512)

    def test_split_operational_limit_and_last_attempt_semantics(self):
        for last in [Trial(rejected=True),Trial(first_certificate=False),Trial(),
                     Trial(events=('hit',)),Trial(exits=True),Trial(whole_clear=True)]:
            schedule=SplitSchedule((last,),attempt=4095)
            self.assertEqual(schedule.run(),direct_transcript((last,),4095))
            before=(schedule.physical,schedule.owner,schedule.attempt,len(schedule.commits))
            schedule.run(5)
            self.assertEqual(before,(schedule.physical,schedule.owner,schedule.attempt,len(schedule.commits)))
        # One operational round can query the first piece but cannot commit it.
        schedule=SplitSchedule((Trial(events=('clear',)*4,exits=True),))
        self.assertEqual(schedule.run(1)[:4],('pending',11,3,1))
        self.assertNotEqual(schedule.phase,4)
        self.assertEqual(schedule.run(),direct_transcript(schedule.trials))
        # All-clear suffix may finish at index==pieces. Failed first admission
        # instead retains pieces==0, with no division or next-piece access.
        schedule=SplitSchedule((Trial(ceiling=frozenset({0}),exits=True),))
        schedule.run(1);self.assertEqual((schedule.index,schedule.pieces),(1,1))
        schedule=SplitSchedule((Trial(first_certificate=False),Trial(exits=True)))
        schedule.run(1);self.assertEqual((schedule.phase,schedule.pieces,schedule.event),(3,0,'uncertain'))
        self.assertEqual(schedule.run(),direct_transcript(schedule.trials))

    def test_split_assembled_helpers_and_entry_order_are_preserved(self):
        prefix=Path('artifacts/optical-normal-selected-split-validation-first')
        modules={label:Path(str(prefix)+'-'+label+'-original.wgsl').read_text()
                 for label in ('producer','certificate','consumer')}
        bodies={label:{name:function(source,name) for name in re.findall(r'\bfn\s+(\w+)\(',source)}
                for label,source in modules.items()}
        entries={'normal_selected_init','normal_selected_produce','normal_selected_consume',
                 'normal_selected_certify','normal_selected_finalize'}
        names=set(bodies['producer'])-entries
        for label in ('certificate','consumer'):
            self.assertEqual(set(bodies[label])-entries,names)
            for name in names:self.assertEqual(bodies['producer'][name],bodies[label][name],(label,name))
        source=Path('scripts/optical_normal_selected_split.rs').read_text()
        self.assertEqual(function(source,'normal_selected_produce'),bodies['producer']['normal_selected_produce'])
        self.assertEqual(function(source,'normal_selected_certify'),bodies['certificate']['normal_selected_certify'])
        for label in modules:
            kept=Path(str(prefix)+'-'+label+'.wgsl').read_text()
            for name in re.findall(r'\bfn\s+(\w+)\(',kept):
                self.assertEqual(function(kept,name),bodies[label][name],(label,name))
        producer=bodies['producer']['normal_selected_produce']
        self.assertNotIn('ns_prepare_piece(',producer)
        self.assertIn('ns.phase==1u||ns.phase==2u||ns.phase==4u',producer)
        self.assertIn('ns.pieces=0u;ns.piece_index=0u;',producer)
        self.assertIn('ns_prepare_piece(ns.pieces==0u)',bodies['certificate']['normal_selected_certify'])
        host=Path('scripts/optical_normal_selected_probe.rs').read_text()
        self.assertLess(host.index('vec!["normal_selected_produce"]'),host.index('repeated_entries.push("normal_selected_certify")'))
        self.assertLess(host.index('repeated_entries.push("normal_selected_certify")'),host.index('repeated_entries.push("normal_selected_consume")'))
        self.assertIn('for &entry in &repeated_entries',host)
        self.assertIn('for _ in 0..prepared.rounds',host)


if __name__=='__main__':unittest.main()
