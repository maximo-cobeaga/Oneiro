//! A minimal Wavefront OBJ reader for props: vertices (`v`) and faces (`f`, any polygon, with or without
//! texture/normal indices, negative indices allowed). Everything else (materials, groups, normals) is ignored;
//! the prop's own `color` paints the model and the toon renderer computes the shading normals.

#[derive(Clone, Debug, Default)]
pub struct ObjModel {
    pub verts: Vec<[f32; 3]>,
    /// Triangles as indices into `verts`.
    pub tris: Vec<[u32; 3]>,
}

impl ObjModel {
    pub fn parse(text: &str) -> Result<ObjModel, String> {
        let mut m = ObjModel::default();
        for (ln, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let n: Vec<f32> = it.take(3).map(|s| s.parse::<f32>()).collect::<Result<_, _>>().map_err(|_| format!("line {}: bad vertex", ln + 1))?;
                    if n.len() < 3 || n.iter().any(|x| !x.is_finite()) {
                        return Err(format!("line {}: a vertex needs three finite numbers", ln + 1));
                    }
                    m.verts.push([n[0], n[1], n[2]]);
                }
                Some("f") => {
                    let mut idx = vec![];
                    for tok in it {
                        let raw: i64 = tok.split('/').next().unwrap_or("").parse().map_err(|_| format!("line {}: bad face index '{}'", ln + 1, tok))?;
                        let i = if raw < 0 { m.verts.len() as i64 + raw } else { raw - 1 };
                        if i < 0 || i as usize >= m.verts.len() {
                            return Err(format!("line {}: face uses vertex {} which does not exist", ln + 1, raw));
                        }
                        idx.push(i as u32);
                    }
                    if idx.len() < 3 {
                        return Err(format!("line {}: a face needs at least three vertices", ln + 1));
                    }
                    for k in 1..idx.len() - 1 {
                        m.tris.push([idx[0], idx[k], idx[k + 1]]);
                    }
                }
                _ => {}
            }
        }
        if m.tris.is_empty() {
            return Err("the model has no faces".into());
        }
        if m.tris.len() > 60_000 {
            return Err(format!("the model has {} triangles; the limit is 60000", m.tris.len()));
        }
        Ok(m)
    }

    /// (min, max) corners.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in &self.verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        (lo, hi)
    }
}
