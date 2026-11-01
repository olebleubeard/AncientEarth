//! Independent exact-dyadic projection/normal tests for the static host ABI.
use super::*;

fn exact(value:i128,shift:u32)->f64{(value as f64)/((1_u64<<shift)as f64)}
fn enclosed(interval:[f64;2],value:f64){assert!(interval[0]<=value&&value<=interval[1],"{interval:?} excludes {value:?}");}
fn dot_integer(a:[i128;3],b:[i128;3])->i128{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn vector(a:[i128;3],shift:u32)->[f64;3]{a.map(|x|exact(x,shift))}
fn cross_integer(a:[i128;3],b:[i128;3])->[i128;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}

#[test]
fn independent_lattice_projection_split_and_eye_motion(){
    let scale=1_i128<<20;
    let origin=[6378137*scale,-200000*scale,100000*scale];
    let matrices=[[[1024,0,0],[0,1024,0],[0,0,1024]],
        [[1024,1,0],[0,1024,1],[0,0,1024]],
        [[2048,0,0],[0,512,0],[0,0,1024]],
        [[0,1024,0],[-1024,0,0],[0,0,1024]]];
    let bases=[[[1,0,0],[0,1,0],[0,0,1]],[[0,1,0],[-1,0,0],[0,0,1]]];
    let mut checked=0;
    for rows in matrices{
        let patch=PatchBasis::new(vector(origin,20),rows.map(|v|vector(v,10))).unwrap();
        for offset in [0,12345*scale+123456,-23456*scale-76543]{
            let a=[origin[0]+offset,origin[1]-offset+13,origin[2]+offset/2-17];
            let ab=[3*scale+1,scale+3,-5];let ac=[7,2*scale+5,4*scale+9];
            let vertices=[a,std::array::from_fn(|i|a[i]+ab[i]),std::array::from_fn(|i|a[i]+ac[i])];
            let face=patch.face(vertices.map(|v|vector(v,20))).unwrap();
            for translation in [[1,-2,3],[scale/2,scale/4,-scale/8],[1000*scale,0,-7*scale]]{
                for columns in bases{
                    let eye_position=std::array::from_fn::<_,3,_>(|i|a[i]+translation[i]);
                    let eye=IdealFrame{origin_ecef:vector(eye_position,20).map(|v|[v,v]),
                        columns:columns.map(|v|v.map(|x|[x as f64,x as f64]))};
                    let encoded_eye=patch.eye(eye).unwrap();
                    let (shift,matrix,b,c)=face_frame(encoded_eye,face);
                    for axis in 0..3{
                        enclosed(shift[axis],exact(dot_integer(rows[axis],translation),30));
                        enclosed(b[axis],exact(dot_integer(rows[axis],ab),30));
                        enclosed(c[axis],exact(dot_integer(rows[axis],ac),30));
                        for column in 0..3{enclosed(matrix[axis][column],exact(dot_integer(rows[axis],columns[column]),10));}
                    }
                    assert_eq!(face.words,patch.face(vertices.map(|v|vector(v,20))).unwrap().words);
                    for row in face.words.into_iter().chain(encoded_eye.words){
                        assert_eq!(row[3].to_bits(),0);
                        assert!(row.into_iter().all(|v|v.is_finite()&&(v==0.||v.abs()>=f32::MIN_POSITIVE)));
                    }
                    checked+=1;
                }
            }
        }
    }
    assert_eq!(checked,72);
}

#[test]
fn nonorthogonal_patch_preserves_plane_with_transposed_normal(){
    // Integer identity A^T[(Au)x(Av)] = det(A)*(u x v), without
    // assuming rows are an orthonormal basis. Positive determinant pins sign.
    for a in [[[2,1,0],[0,3,1],[0,0,2]],[[1,0,2],[1,2,0],[0,0,3]]]{
        let u=[3,-2,1];let v=[1,4,-3];
        let determinant=dot_integer(a[0],cross_integer(a[1],a[2]));assert!(determinant>0);
        let patch_normal=cross_integer(a.map(|r|dot_integer(r,u)),a.map(|r|dot_integer(r,v)));
        let transformed=std::array::from_fn::<_,3,_>(|i|a[0][i]*patch_normal[0]+a[1][i]*patch_normal[1]+a[2][i]*patch_normal[2]);
        assert_eq!(transformed,cross_integer(u,v).map(|n|determinant*n));
        let patch=PatchBasis::new([0.;3],a.map(|r|r.map(|v|v as f64))).unwrap();
        assert!(patch.determinant()[0]>0.);
    }
    assert!(PatchBasis::new([0.;3],[[1.,0.,0.],[0.,1.,0.],[0.,0.,-1.]]).is_err());
    assert!(PatchBasis::new([0.;3],[[1.,2.,3.],[2.,4.,6.],[0.,0.,1.]]).is_err());
    assert!(PatchBasis::new([f64::NAN,0.,0.],[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]).is_err());
}
