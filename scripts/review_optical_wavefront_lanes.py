"""Count-only lane-spread audit; an equal-cost model, not measured GPU work."""
import argparse
from collections import Counter
import json
from pathlib import Path

import numpy as np
from review_optical_wavefront import read, distribution


def review(prefix):
    report,terminal,detail,_,_=read(prefix,True)
    assert report['reference'] and not report['selected']
    counts=detail[:,0].astype(np.uint64);assert len(counts)%32==0
    groups=counts.reshape(-1,32);maxima=groups.max(axis=1);minima=groups.min(axis=1)
    attempts=int(counts.sum());slots=int(maxima.sum())*32
    per_round=[]
    for start in range(0,4096,32):
        work=np.minimum(np.maximum(groups.astype(np.int64)-start,0),32)
        live=work>0
        per_round.append({'completed_before_slice':start,'live_rays':int(live.sum()),
                          'live_original_groups':int(np.count_nonzero(live.any(axis=1))),
                          'complete_attempts':int(work.sum()),
                          'equal_cost_original_group_slots':int(work.max(axis=1).sum())*32})
    return {'scope':__doc__, 'prefix':prefix,'records':len(counts),'group_width':32,
            'group_count':len(groups),'attempts':attempts,'maximum_per_group_slot_attempts':slots,
            'equal_attempt_cost_idle_fraction':(slots-attempts)/slots,
            'equal_attempt_cost_ideal_speedup_ceiling':slots/attempts,
            'within_group_attempt_range':distribution(maxima-minima),
            'group_max_attempt_distribution':distribution(maxima),
            'attempt_count_histogram':dict(Counter(map(int,counts))),
            'group_range_histogram':dict(Counter(map(int,maxima-minima))),
            'all32_counts_equal_groups':int(np.count_nonzero(maxima==minima)),
            'mixed_terminal_status_groups':int(np.count_nonzero(np.any(terminal[:,7].reshape(-1,32)!=terminal[:,7].reshape(-1,32)[:,:1],axis=1))),
            'slices32':per_round,
            'limitations':'Group32 is the declared workgroup and recorded linear pixel grouping. Hardware '
                          'subgroup scheduling and per-attempt cost can differ. This counts whole outer '
                          'attempts, including retries, but cannot price inner root/BVH work, registers, '
                          'memory traffic or dispatches. The ideal ratio assumes equal attempt cost and '
                          'free arbitrary lane packing; it is not a general speedup bound.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');args=parser.parse_args()
    output=Path(args.prefix+'-lane-review.json');assert not output.exists()
    result=review(args.prefix);output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ['attempt_count_histogram','group_range_histogram','slices32']},indent=2))
