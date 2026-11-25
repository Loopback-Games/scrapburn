//! Minimal binary glTF (GLB) reader: triangle meshes with positions, normals, UVs, node
//! transforms, and the base color texture. Enough for prop and vehicle models.

use std::path::Path;

use serde_json::Value;

use crate::image_data::Rgba8Image;

#[derive(Debug, thiserror::Error)]
pub enum GlbError {
    #[error("reading {0}: {1}")]
    Io(String, std::io::Error),
    #[error("not a GLB v2 file")]
    NotGlb,
    #[error("malformed glTF: {0}")]
    Malformed(String),
    #[error("decoding embedded image: {0}")]
    Image(#[from] image::ImageError),
}

fn bad(msg: impl Into<String>) -> GlbError {
    GlbError::Malformed(msg.into())
}

/// All triangles of the default scene, merged, in model space (node transforms applied).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlbMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// The first material's base color texture, if any.
    pub base_color: Option<Rgba8Image>,
}

pub fn load_glb(path: &Path) -> Result<GlbMesh, GlbError> {
    let bytes = std::fs::read(path).map_err(|e| GlbError::Io(path.display().to_string(), e))?;
    parse_glb(&bytes)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, GlbError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(GlbError::NotGlb)
}

struct Doc<'a> {
    json: Value,
    bin: &'a [u8],
}

impl Doc<'_> {
    fn arr(&self, key: &str) -> &[Value] {
        self.json
            .get(key)
            .and_then(Value::as_array)
            .map_or(&[], |v| v.as_slice())
    }

    fn view(&self, idx: usize) -> Result<(&[u8], usize), GlbError> {
        let v = self
            .arr("bufferViews")
            .get(idx)
            .ok_or_else(|| bad("bufferView index"))?;
        let off = v.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let len = v
            .get("byteLength")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("byteLength"))? as usize;
        let stride = v.get("byteStride").and_then(Value::as_u64).unwrap_or(0) as usize;
        let data = self
            .bin
            .get(off..off + len)
            .ok_or_else(|| bad("bufferView out of range"))?;
        Ok((data, stride))
    }

    /// Reads an accessor as f32 components (`n` per element) or integer indices.
    fn floats(&self, idx: usize, n: usize) -> Result<Vec<f32>, GlbError> {
        let a = self
            .arr("accessors")
            .get(idx)
            .ok_or_else(|| bad("accessor index"))?;
        if a.get("componentType").and_then(Value::as_u64) != Some(5126) {
            return Err(bad("expected float accessor"));
        }
        let count = a
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("count"))? as usize;
        let (data, stride) = self.view(
            a.get("bufferView")
                .and_then(Value::as_u64)
                .ok_or_else(|| bad("sparse accessors unsupported"))? as usize,
        )?;
        let base = a.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let stride = if stride == 0 { n * 4 } else { stride };
        let mut out = Vec::with_capacity(count * n);
        for i in 0..count {
            for c in 0..n {
                let at = base + i * stride + c * 4;
                let s = data
                    .get(at..at + 4)
                    .ok_or_else(|| bad("accessor out of range"))?;
                out.push(f32::from_le_bytes([s[0], s[1], s[2], s[3]]));
            }
        }
        Ok(out)
    }

    fn indices(&self, idx: usize) -> Result<Vec<u32>, GlbError> {
        let a = self
            .arr("accessors")
            .get(idx)
            .ok_or_else(|| bad("accessor index"))?;
        let count = a
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("count"))? as usize;
        let ct = a
            .get("componentType")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("componentType"))?;
        let (data, _) = self.view(
            a.get("bufferView")
                .and_then(Value::as_u64)
                .ok_or_else(|| bad("bufferView"))? as usize,
        )?;
        let base = a.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let size = match ct {
            5121 => 1,
            5123 => 2,
            5125 => 4,
            _ => return Err(bad("index component type")),
        };
        (0..count)
            .map(|i| {
                let at = base + i * size;
                let s = data
                    .get(at..at + size)
                    .ok_or_else(|| bad("indices out of range"))?;
                Ok(match size {
                    1 => s[0] as u32,
                    2 => u16::from_le_bytes([s[0], s[1]]) as u32,
                    _ => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                })
            })
            .collect()
    }

    fn base_color(&self) -> Result<Option<Rgba8Image>, GlbError> {
        let tex = self
            .arr("materials")
            .first()
            .and_then(|m| m.pointer("/pbrMetallicRoughness/baseColorTexture/index"))
            .and_then(Value::as_u64);
        let Some(tex) = tex else { return Ok(None) };
        let source = self
            .arr("textures")
            .get(tex as usize)
            .and_then(|t| t.get("source"))
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("texture source"))?;
        let img = self
            .arr("images")
            .get(source as usize)
            .ok_or_else(|| bad("image index"))?;
        let view = img
            .get("bufferView")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("external images unsupported"))?;
        let (data, _) = self.view(view as usize)?;
        let decoded = image::load_from_memory(data)?.into_rgba8();
        Ok(Some(Rgba8Image {
            width: decoded.width(),
            height: decoded.height(),
            pixels: decoded.into_raw(),
        }))
    }
}

/// Column-major 4×4 transform of a node (matrix, or translation/rotation/scale).
fn node_matrix(node: &Value) -> [f32; 16] {
    if let Some(m) = node.get("matrix").and_then(Value::as_array)
        && m.len() == 16
    {
        return std::array::from_fn(|i| m[i].as_f64().unwrap_or(0.0) as f32);
    }
    let get = |k: &str, n: usize, d: &[f32]| -> Vec<f32> {
        node.get(k)
            .and_then(Value::as_array)
            .map_or(d.to_vec(), |a| {
                (0..n)
                    .map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(d[i] as f64) as f32)
                    .collect()
            })
    };
    let t = get("translation", 3, &[0.0, 0.0, 0.0]);
    let r = get("rotation", 4, &[0.0, 0.0, 0.0, 1.0]);
    let s = get("scale", 3, &[1.0, 1.0, 1.0]);
    let (x, y, z, w) = (r[0], r[1], r[2], r[3]);
    let rot = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [
        rot[0] * s[0],
        rot[1] * s[0],
        rot[2] * s[0],
        0.0,
        rot[3] * s[1],
        rot[4] * s[1],
        rot[5] * s[1],
        0.0,
        rot[6] * s[2],
        rot[7] * s[2],
        rot[8] * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| {
        let (col, row) = (i / 4, i % 4);
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}

fn xform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn xform_dir(m: &[f32; 16], d: [f32; 3]) -> [f32; 3] {
    let v: [f32; 3] = std::array::from_fn(|r| m[r] * d[0] + m[4 + r] * d[1] + m[8 + r] * d[2]);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
    [v[0] / len, v[1] / len, v[2] / len]
}

pub fn parse_glb(bytes: &[u8]) -> Result<GlbMesh, GlbError> {
    if bytes.get(0..4) != Some(b"glTF") || u32_at(bytes, 4)? != 2 {
        return Err(GlbError::NotGlb);
    }
    let json_len = u32_at(bytes, 12)? as usize;
    if bytes.get(16..20) != Some(b"JSON") {
        return Err(GlbError::NotGlb);
    }
    let json_bytes = bytes.get(20..20 + json_len).ok_or(GlbError::NotGlb)?;
    let json: Value = serde_json::from_slice(json_bytes).map_err(|e| bad(e.to_string()))?;
    let bin_at = 20 + json_len;
    let bin = if bytes.len() >= bin_at + 8 && bytes.get(bin_at + 4..bin_at + 8) == Some(b"BIN\0") {
        let len = u32_at(bytes, bin_at)? as usize;
        bytes
            .get(bin_at + 8..bin_at + 8 + len)
            .ok_or_else(|| bad("BIN chunk length"))?
    } else {
        &[][..]
    };
    let doc = Doc { json, bin };

    let mut out = GlbMesh::default();
    let scene = doc.json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let roots: Vec<usize> = doc
        .arr("scenes")
        .get(scene)
        .and_then(|s| s.get("nodes"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_u64)
                .map(|n| n as usize)
                .collect()
        })
        .unwrap_or_else(|| (0..doc.arr("nodes").len()).collect());
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut stack: Vec<(usize, [f32; 16])> = roots.into_iter().map(|n| (n, identity)).collect();
    let mut guard = 0;
    while let Some((ni, parent)) = stack.pop() {
        guard += 1;
        if guard > 100_000 {
            return Err(bad("node graph too large or cyclic"));
        }
        let node = doc.arr("nodes").get(ni).ok_or_else(|| bad("node index"))?;
        let world = mul(&parent, &node_matrix(node));
        if let Some(mi) = node.get("mesh").and_then(Value::as_u64) {
            let mesh = doc
                .arr("meshes")
                .get(mi as usize)
                .ok_or_else(|| bad("mesh index"))?;
            for prim in mesh
                .get("primitives")
                .and_then(Value::as_array)
                .map_or(&[][..], |v| v.as_slice())
            {
                if prim.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                    continue; // triangles only
                }
                let attrs = prim.get("attributes").ok_or_else(|| bad("attributes"))?;
                let pos_i = attrs
                    .get("POSITION")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| bad("POSITION"))? as usize;
                let pos = doc.floats(pos_i, 3)?;
                let count = pos.len() / 3;
                let nrm = match attrs.get("NORMAL").and_then(Value::as_u64) {
                    Some(i) => doc.floats(i as usize, 3)?,
                    None => [0.0, 1.0, 0.0].repeat(count),
                };
                let uv = match attrs.get("TEXCOORD_0").and_then(Value::as_u64) {
                    Some(i) => doc.floats(i as usize, 2)?,
                    None => vec![0.0; count * 2],
                };
                let base = out.positions.len() as u32;
                for v in 0..count {
                    out.positions.push(xform_point(
                        &world,
                        [pos[v * 3], pos[v * 3 + 1], pos[v * 3 + 2]],
                    ));
                    out.normals.push(xform_dir(
                        &world,
                        [nrm[v * 3], nrm[v * 3 + 1], nrm[v * 3 + 2]],
                    ));
                    out.uvs.push([uv[v * 2], uv[v * 2 + 1]]);
                }
                match prim.get("indices").and_then(Value::as_u64) {
                    Some(i) => out
                        .indices
                        .extend(doc.indices(i as usize)?.into_iter().map(|x| x + base)),
                    None => out.indices.extend(base..base + count as u32),
                }
            }
        }
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            stack.extend(
                children
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|c| (c as usize, world)),
            );
        }
    }
    out.base_color = doc.base_color()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GLB with one triangle (u16 indices) under a node translated by (0, 0, 5).
    fn tiny_glb() -> Vec<u8> {
        let mut bin = Vec::new();
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in v {
                bin.extend(c.to_le_bytes());
            }
        }
        for n in [[0.0f32, 0.0, 1.0]; 3] {
            for c in n {
                bin.extend(c.to_le_bytes());
            }
        }
        for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for c in uv {
                bin.extend(c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bin.extend(i.to_le_bytes());
        }
        bin.extend([0, 0]); // pad to 4
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0, "translation": [0.0, 0.0, 5.0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}, "indices": 3}]}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 36},
                {"buffer": 0, "byteOffset": 72, "byteLength": 24},
                {"buffer": 0, "byteOffset": 96, "byteLength": 6}
            ],
            "buffers": [{"byteLength": bin.len()}]
        });
        let mut js = serde_json::to_vec(&json).unwrap();
        while !js.len().is_multiple_of(4) {
            js.push(b' ');
        }
        let total = 12 + 8 + js.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((js.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(&js);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(&bin);
        out
    }

    #[test]
    fn parses_tiny_glb_with_node_transform() {
        let m = parse_glb(&tiny_glb()).unwrap();
        assert_eq!(m.indices, vec![0, 1, 2]);
        assert_eq!(m.positions[1], [1.0, 0.0, 5.0]);
        assert_eq!(m.normals[0], [0.0, 0.0, 1.0]);
        assert_eq!(m.uvs[2], [0.0, 1.0]);
        assert!(m.base_color.is_none());
    }

    #[test]
    fn rejects_non_glb() {
        assert!(parse_glb(b"not a glb file at all").is_err());
    }
}
