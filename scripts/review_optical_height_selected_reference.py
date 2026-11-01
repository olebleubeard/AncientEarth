"""Independent actual-launch references for the selected hybrid tail inputs."""
import json
from pathlib import Path
import sys

import numpy as np

from review_optical_height_selected import review
from verify_optical_dop853 import A, differences, geometry, integrate


def audit(prefix, full_prefix):
    checked = review(prefix, full_prefix)
    source = json.loads(Path(prefix + '.json').read_text())
    original = source['fixture']['clear_ray_fixture']['cases'][0]['canonical_inputs']
    origin = np.array(original['origin_ecef_m'])
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.])
    east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    # The new optional terminal policy uses the existing directed event. An
    # equatorial radial vacuum ray gives an independent exact boundary check.
    radial = dict(origin_ecef_m=[A + 100., 0., 0.], initial_direction=[-1., 0., 0.],
                  layers=[dict(height_m=0., refractivity=0.), dict(height_m=120000., refractivity=0.)])
    radial_result = integrate(radial, True, stop_at_lower_domain=True)
    assert radial_result['terminal_kind'] == 'lower_domain'
    assert abs(radial_result['geometric_path_m'] - 10100.) < 1e-8
    assert np.linalg.norm(np.array(radial_result['endpoint_ecef_m']) - [A - 10000., 0., 0.]) < 1e-8
    rows = []
    for row in checked['records']:
        case = dict(original, initial_direction=(basis @ row['launch_local']).tolist())
        case.pop('target_path_m', None)
        coarse = integrate(case, False, stop_at_lower_domain=True)
        reference = integrate(case, True, stop_at_lower_domain=True)
        assert coarse['terminal_kind'] == reference['terminal_kind']
        raw = np.array(row['raw_words'], dtype='<u4').view('<f4')
        observed = dict(endpoint_ecef_m=(origin + basis @ raw[:3]).tolist(),
                        exit_direction=(basis @ raw[4:7]).tolist(), geometric_path_m=float(raw[3]))
        error = dict(endpoint_m=float(np.linalg.norm(np.array(observed['endpoint_ecef_m']) - reference['endpoint_ecef_m'])),
                     direction=float(np.linalg.norm(np.array(observed['exit_direction']) - reference['exit_direction'])),
                     path_m=abs(observed['geometric_path_m'] - reference['geometric_path_m']))
        completed = row['status'] == 0 and reference['terminal_kind'] == 'atmosphere_exit'
        expected_rejection = row['status'] == 2 and row['reason'] == 5 and reference['terminal_kind'] == 'lower_domain'
        assert completed or expected_rejection, (row, reference['terminal_kind'])
        result = dict(pixel=row['pixel'], status=row['status'], reason=row['reason'], attempts=row['attempts'],
                      canonical_inputs=case, reference=reference,
                      convergence=differences(coarse, reference),
                      observed_geodetic_height_m=float(geometry(np.array(observed['endpoint_ecef_m']))[0]),
                      reference_terminal_matches=True)
        if completed:
            result['completed_exit_difference'] = error
        else:
            result['rejected_prefix_to_reference_boundary_difference'] = error
            result['scope'] = 'A lower-domain rejection is not a completed optical exit; prefix differences are diagnostic boundary-policy evidence only.'
        rows.append(result)
        print(json.dumps({k: v for k, v in result.items() if k not in ('reference', 'canonical_inputs')}), flush=True)
    exits = [row['completed_exit_difference'] for row in rows if 'completed_exit_difference' in row]
    return dict(scope='Independent canonical WGS84 DOP853 references use every selected exact GPU launch in the original canonical eye frame. Lower-domain rejection is a separate terminal policy. No scene contacts, optical-path GPU comparison, global error proof or native throughput claim.',
                raw_audit=prefix + '-independent-review.json', radial_lower_boundary_check=radial_result,
                cases=rows, completed_exits=len(exits), lower_domain_rejections=len(rows)-len(exits),
                maximum_completed_exit_difference={key: max(row[key] for row in exits) for key in ['endpoint_m','direction','path_m']},
                maximum_convergence={key: max(row['convergence'][key] for row in rows) for key in ['endpoint_m','direction','path_m','optical_path_m']})


if __name__ == '__main__':
    prefix, baseline = sys.argv[1:3]
    result = audit(prefix, baseline)
    Path(prefix + '-reference-review.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ('cases', 'radial_lower_boundary_check')}, indent=2))
