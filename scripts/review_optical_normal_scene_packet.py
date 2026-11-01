"""Independent byte/layout audit of the canonical scene preparation.

The authored outward all-vertex preparation and separately reviewed ideal-frame
theorem supply numerical face enclosures. This byte audit checks their retained
layout and every permitted mutation; it does not infer source geometry from
nearby GPU vertices or claim universal scene acceptance.
"""
import hashlib
import json
from pathlib import Path
import numpy as np


def review():
    prefix='artifacts/optical-normal-scene-packet'
    metadata=json.loads(Path(prefix+'.json').read_text())
    provenance='artifacts/optical-normal-scene-preparation-first'
    assert json.loads(Path(provenance+'-input-stability.json').read_text(encoding='utf-8-sig'))['inputs_unchanged_through_run']
    for item in json.loads(Path(provenance+'-outputs.json').read_text(encoding='utf-8-sig')):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256'],item['path']
    original=np.fromfile(metadata['retained_geometry'],dtype='<u4').reshape(-1,4)
    geometry=np.fromfile(prefix+'-geometry.bin',dtype='<u4').reshape(-1,4)
    assert original.shape==geometry.shape
    assert Path(prefix+'-frames.bin').read_bytes()==Path(metadata['retained_frames']).read_bytes()
    cert_words=np.fromfile(prefix+'-certificates.bin',dtype='<u4').reshape(-1,4)
    cert=cert_words.view('<f4')
    assert len(cert)==metadata['certificate_vec4_count'] and np.isfinite(cert).all()
    patch_count,marker_count,layer_count,patch_start=map(int,cert_words[0])
    marker_start,source_start,face_start,version=map(int,cert_words[1])
    assert version==1 and patch_count==len(metadata['patches'])==11
    assert marker_count==len(metadata['marker_records'])==metadata['markers']==81
    assert source_start==5 and patch_start==5+layer_count
    assert marker_start==patch_start+9*patch_count and face_start==marker_start+3*marker_count
    assert np.all(cert[source_start:patch_start,0]<=cert[source_start:patch_start,1])
    assert np.all(cert[source_start:patch_start,2:]==0)
    assert np.all(cert[2]<=cert[3]) and cert[4,0]<=cert[4,1]
    assert float(cert[4,2])==metadata['canonical_height_ceiling_m']
    def vectors(start,end):
        section=cert[start:end]
        assert len(section)%2==0
        assert np.all(section[0::2,:3]<=section[1::2,:3])
        assert np.all(section[:,3]==0)
    changed=np.zeros(len(geometry),dtype=np.bool_)
    cursor=face_start
    count=0
    for index,patch in enumerate(metadata['patches']):
        root,triangles,base=patch['root'],patch['triangles'],patch['metadata_vec4']
        assert patch['patch']==index and base==patch_start+9*index
        first_triangle=int(original[root,3])
        assert first_triangle>root and (first_triangle-root)%2==0
        assert tuple(map(int,cert_words[base,1:3]))==(root,first_triangle)
        assert float(cert[base,0])==patch['source_component_error_m']>0
        vectors(base+1,base+9)
        nodes=original[root:first_triangle].view('<f4')
        assert np.isfinite(nodes[:,:3]).all() and np.all(nodes[0::2,:3]<=nodes[1::2,:3])
        addresses=first_triangle+3*np.arange(triangles,dtype=np.int64)
        expected=cursor+6*np.arange(triangles,dtype=np.uint32)
        assert np.array_equal(geometry[addresses,3],expected)
        assert not changed[addresses].any()
        changed[addresses]=True
        vectors(cursor,cursor+6*triangles)
        cursor+=6*triangles;count+=triangles
    assert count==metadata['triangles']==618560
    # Exactly these first-vertex payload words may differ; all xyz, BVH links,
    # second/third vertex payloads and other geometry words stay bit-identical.
    differences=geometry!=original
    assert not differences[:,:3].any()
    assert np.array_equal(differences[:,3],changed)
    for marker,record in enumerate(metadata['marker_records']):
        base=marker_start+3*marker
        assert record['marker']==marker and record['face_certificates_vec4']==cursor
        assert tuple(map(int,cert_words[base,:2]))==(cursor,marker)
        assert np.all(cert[base+1,:3]<=cert[base+2,:3])
        vectors(cursor,cursor+84);cursor+=84
    assert cursor==len(cert)
    return dict(passed=True,terrain_faces=count,marker_faces=81*14,
                changed_geometry_words=int(differences.sum()),geometry_xyz_and_topology_exact=True,
                retained_frames_exact=True,certificate_bytes=int(cert.nbytes),all_vector_endpoints_finite_ordered=True,
                source_component_error_range_m=[min(p['source_component_error_m'] for p in metadata['patches']),max(p['source_component_error_m'] for p in metadata['patches'])],
                canonical_eye_minus_supplied_eye_interval_m=metadata['ideal_minus_supplied_eye_ecef_interval_m'],
                scope=__doc__.strip())


if __name__=='__main__':
    result=review();destination=Path('artifacts/optical-normal-scene-packet-independent-review.json')
    assert not destination.exists(),'retain earlier evidence'
    destination.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result,indent=2))
