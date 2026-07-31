use pcd_rs::{DataKind, DynReader, DynRecord, Field, PcdMeta, ValueKind};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcdEncoding {
    Ascii,
    Binary,
}

impl Display for PcdEncoding {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ascii => "ASCII",
            Self::Binary => "Binary",
        })
    }
}

#[derive(Debug)]
pub struct LoadedPcd {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 4]>,
    pub skipped_points: u64,
    pub declared_points: u64,
    pub encoding: PcdEncoding,
}

#[derive(Debug)]
pub struct PcdLoadError(String);

impl Display for PcdLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PcdLoadError {}

#[derive(Clone, Copy)]
enum ColorSource {
    Packed {
        index: usize,
        alpha: bool,
    },
    Separate {
        r: usize,
        g: usize,
        b: usize,
        a: Option<usize>,
    },
    Intensity(usize),
    Height,
}

struct LoadBuffers {
    positions: Vec<[f32; 3]>,
    colors: Vec<[u8; 4]>,
    fallback_values: Vec<f32>,
}

pub fn load_pcd(
    path: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<LoadedPcd, PcdLoadError> {
    let meta = PcdMeta::from_path(path).map_err(|error| PcdLoadError(error.to_string()))?;
    let encoding = match meta.data {
        DataKind::Ascii => PcdEncoding::Ascii,
        DataKind::Binary => PcdEncoding::Binary,
        DataKind::BinaryCompressed => {
            return Err(PcdLoadError(
                "unsupported PCD encoding binary_compressed".to_owned(),
            ));
        }
    };
    let declared_points = meta.num_points;
    let fields = meta
        .field_defs
        .iter()
        .filter(|field| !field.is_padding())
        .collect::<Vec<_>>();
    let names = fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>();
    let field_index = |name: &str| names.iter().position(|candidate| *candidate == name);
    let required = [field_index("x"), field_index("y"), field_index("z")];
    let [Some(x), Some(y), Some(z)] = required else {
        return Err(PcdLoadError(format!(
            "required fields x, y, z; discovered fields: {}",
            names.join(", ")
        )));
    };
    for (name, index) in [("x", x), ("y", y), ("z", z)] {
        if fields[index].count != 1 {
            return Err(PcdLoadError(format!(
                "field {name} must be scalar, found count {}",
                fields[index].count
            )));
        }
    }

    let color_source = if let Some(index) = field_index("rgba") {
        scalar_field(&fields, index, "rgba")?;
        ColorSource::Packed { index, alpha: true }
    } else if let Some(index) = field_index("rgb") {
        scalar_field(&fields, index, "rgb")?;
        ColorSource::Packed {
            index,
            alpha: false,
        }
    } else if let (Some(r), Some(g), Some(b)) =
        (field_index("r"), field_index("g"), field_index("b"))
    {
        for (name, index) in [("r", r), ("g", g), ("b", b)] {
            scalar_field(&fields, index, name)?;
        }
        let a = field_index("a");
        if let Some(index) = a {
            scalar_field(&fields, index, "a")?;
        }
        ColorSource::Separate { r, g, b, a }
    } else if let Some(index) = field_index("intensity") {
        scalar_field(&fields, index, "intensity")?;
        ColorSource::Intensity(index)
    } else {
        ColorSource::Height
    };
    if encoding == PcdEncoding::Ascii {
        if let ColorSource::Packed { index, .. } = color_source {
            if fields[index].kind == ValueKind::F32 {
                return Err(PcdLoadError(format!(
                    "ASCII packed {}:F32 cannot preserve color payload bits (including NaN alpha patterns); use U32 or I32 for ASCII, or use binary PCD for packed F32 colors",
                    fields[index].name
                )));
            }
        }
    }

    let LoadBuffers {
        mut positions,
        mut colors,
        mut fallback_values,
    } = allocate_buffers(declared_points, color_source)?;
    let mut skipped_points = 0;
    let reader = DynReader::open(path).map_err(|error| PcdLoadError(error.to_string()))?;
    progress(0, declared_points);
    for (point_index, record) in reader.enumerate() {
        let record = record.map_err(|error| {
            PcdLoadError(format!("failed to parse point {point_index}: {error}"))
        })?;
        let position = [
            scalar_f32(&record, x, "x")?,
            scalar_f32(&record, y, "y")?,
            scalar_f32(&record, z, "z")?,
        ];
        if position.iter().all(|coordinate| coordinate.is_finite()) {
            positions.push(position);
            match color_source {
                ColorSource::Packed { index, alpha } => {
                    colors.push(packed_color(&record.0[index], alpha, index)?);
                }
                ColorSource::Separate { r, g, b, a } => colors.push([
                    color_channel(&record, r, "r")?,
                    color_channel(&record, g, "g")?,
                    color_channel(&record, b, "b")?,
                    a.map(|index| color_channel(&record, index, "a"))
                        .transpose()?
                        .unwrap_or(255),
                ]),
                ColorSource::Intensity(index) => {
                    fallback_values.push(scalar_f32(&record, index, "intensity")?);
                }
                ColorSource::Height => fallback_values.push(position[2]),
            }
        } else {
            skipped_points += 1;
        }
        progress(point_index as u64 + 1, declared_points);
    }

    if matches!(
        color_source,
        ColorSource::Intensity(_) | ColorSource::Height
    ) {
        colors = normalized_colors(
            &fallback_values,
            matches!(color_source, ColorSource::Height),
        )?;
    }

    Ok(LoadedPcd {
        positions,
        colors,
        skipped_points,
        declared_points,
        encoding,
    })
}

fn allocate_buffers(
    declared_points: u64,
    color_source: ColorSource,
) -> Result<LoadBuffers, PcdLoadError> {
    let capacity = usize::try_from(declared_points).map_err(|_| {
        PcdLoadError(format!(
            "cannot reserve buffers for {declared_points} declared points: count exceeds platform capacity"
        ))
    })?;
    let mut positions = Vec::new();
    positions.try_reserve(capacity).map_err(|error| {
        PcdLoadError(format!(
            "cannot reserve positions for {declared_points} declared points: {error}"
        ))
    })?;
    let mut colors = Vec::new();
    let mut fallback_values = Vec::new();
    if matches!(
        color_source,
        ColorSource::Intensity(_) | ColorSource::Height
    ) {
        fallback_values.try_reserve(capacity).map_err(|error| {
            PcdLoadError(format!(
                "cannot reserve fallback values for {declared_points} declared points: {error}"
            ))
        })?;
    } else {
        colors.try_reserve(capacity).map_err(|error| {
            PcdLoadError(format!(
                "cannot reserve colors for {declared_points} declared points: {error}"
            ))
        })?;
    }
    Ok(LoadBuffers {
        positions,
        colors,
        fallback_values,
    })
}

fn scalar_field(
    fields: &[&pcd_rs::FieldDef],
    index: usize,
    name: &str,
) -> Result<(), PcdLoadError> {
    if fields[index].count == 1 {
        Ok(())
    } else {
        Err(PcdLoadError(format!(
            "field {name} must be scalar, found count {}",
            fields[index].count
        )))
    }
}

fn scalar_f32(record: &DynRecord, index: usize, name: &str) -> Result<f32, PcdLoadError> {
    let value = match &record.0[index] {
        Field::I8(values) => values.first().map(|value| *value as f32),
        Field::I16(values) => values.first().map(|value| *value as f32),
        Field::I32(values) => values.first().map(|value| *value as f32),
        Field::I64(values) => values.first().map(|value| *value as f32),
        Field::U8(values) => values.first().map(|value| *value as f32),
        Field::U16(values) => values.first().map(|value| *value as f32),
        Field::U32(values) => values.first().map(|value| *value as f32),
        Field::U64(values) => values.first().map(|value| *value as f32),
        Field::F32(values) => values.first().copied(),
        Field::F64(values) => values.first().map(|value| *value as f32),
    };
    value.ok_or_else(|| PcdLoadError(format!("field {name} is not scalar")))
}

fn color_channel(record: &DynRecord, index: usize, name: &str) -> Result<u8, PcdLoadError> {
    Ok(scalar_f32(record, index, name)?.round().clamp(0.0, 255.0) as u8)
}

fn packed_color(field: &Field, alpha: bool, index: usize) -> Result<[u8; 4], PcdLoadError> {
    let bits = match field {
        Field::F32(values) => values.first().map(|value| value.to_bits()),
        Field::U32(values) => values.first().copied(),
        Field::I32(values) => values.first().map(|value| *value as u32),
        _ => None,
    }
    .ok_or_else(|| {
        PcdLoadError(format!(
            "packed color field at schema index {index} must be scalar F32, U32, or I32"
        ))
    })?;
    Ok([
        ((bits >> 16) & 0xff) as u8,
        ((bits >> 8) & 0xff) as u8,
        (bits & 0xff) as u8,
        if alpha { (bits >> 24) as u8 } else { 255 },
    ])
}

fn normalized_colors(values: &[f32], height: bool) -> Result<Vec<[u8; 4]>, PcdLoadError> {
    let finite = values.iter().copied().filter(|value| value.is_finite());
    let (minimum, maximum) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |range, value| {
        (range.0.min(value), range.1.max(value))
    });
    let mut colors = Vec::new();
    colors.try_reserve(values.len()).map_err(|error| {
        PcdLoadError(format!(
            "cannot reserve fallback colors for {} valid points: {error}",
            values.len()
        ))
    })?;
    colors.extend(values.iter().map(|value| {
        let normalized = if value.is_finite() && maximum > minimum {
            (*value - minimum) / (maximum - minimum)
        } else {
            0.5
        };
        let channel = (normalized * 255.0).round() as u8;
        if height {
            [channel, 0, ((1.0 - normalized) * 255.0).round() as u8, 255]
        } else {
            [channel, channel, channel, 255]
        }
    }));
    Ok(colors)
}

#[cfg(test)]
mod tests {
    use super::{allocate_buffers, load_pcd, ColorSource, PcdEncoding};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn ascii_fixture(
        fields: &str,
        size: &str,
        field_type: &str,
        count: &str,
        points: u64,
        data: &str,
    ) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "# .PCD v0.7\nVERSION 0.7\nFIELDS {fields}\nSIZE {size}\nTYPE {field_type}\nCOUNT {count}\nWIDTH {points}\nHEIGHT 1\nVIEWPOINT 0 0 0 1 0 0 0\nPOINTS {points}\nDATA ascii\n{data}"
        )
        .unwrap();
        file
    }

    fn binary_fixture(
        fields: &str,
        size: &str,
        field_type: &str,
        points: u64,
        records: &[u8],
    ) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "VERSION 0.7\nFIELDS {fields}\nSIZE {size}\nTYPE {field_type}\nCOUNT {}\nWIDTH {points}\nHEIGHT 1\nPOINTS {points}\nDATA binary\n",
            fields.split_whitespace().map(|_| "1").collect::<Vec<_>>().join(" ")
        )
        .unwrap();
        file.write_all(records).unwrap();
        file
    }

    fn xyz_bytes(points: &[[f32; 3]]) -> Vec<u8> {
        points
            .iter()
            .flat_map(|point| point.iter().flat_map(|value| value.to_le_bytes()))
            .collect()
    }

    #[test]
    fn loads_ascii_xyz_and_reports_progress() {
        let file = ascii_fixture("x y z", "4 4 4", "F F F", "1 1 1", 2, "1 2 3\n-4 5 6\n");
        let mut progress = Vec::new();

        let loaded = load_pcd(file.path(), |done, total| progress.push((done, total))).unwrap();

        assert_eq!(loaded.positions, vec![[1.0, 2.0, 3.0], [-4.0, 5.0, 6.0]]);
        assert_eq!(loaded.colors, vec![[0, 0, 255, 255], [255, 0, 0, 255]]);
        assert_eq!(loaded.declared_points, 2);
        assert_eq!(loaded.skipped_points, 0);
        assert_eq!(loaded.encoding, PcdEncoding::Ascii);
        assert_eq!(progress.last(), Some(&(2, 2)));
    }

    #[test]
    fn loads_binary_xyz() {
        let bytes = xyz_bytes(&[[1.25, -2.5, 3.75], [4.0, 5.0, 6.0]]);
        let file = binary_fixture("x y z", "4 4 4", "F F F", 2, &bytes);

        let loaded = load_pcd(file.path(), |_, _| {}).unwrap();

        assert_eq!(loaded.positions, vec![[1.25, -2.5, 3.75], [4.0, 5.0, 6.0]]);
        assert_eq!(loaded.encoding, PcdEncoding::Binary);
    }

    #[test]
    fn decodes_ascii_packed_integer_rgb_and_rgba_bit_patterns() {
        for (field, field_type, packed, expected) in [
            (
                "rgb",
                "U",
                u32::from_be_bytes([0, 0x12, 0x34, 0x56]),
                [0x12, 0x34, 0x56, 255],
            ),
            (
                "rgba",
                "I",
                u32::from_be_bytes([0x78, 0x12, 0x34, 0x56]),
                [0x12, 0x34, 0x56, 0x78],
            ),
        ] {
            let data = if field_type == "I" {
                format!("1 2 3 {}\n", packed as i32)
            } else {
                format!("1 2 3 {packed}\n")
            };
            let file = ascii_fixture(
                &format!("x y z {field}"),
                "4 4 4 4",
                &format!("F F F {field_type}"),
                "1 1 1 1",
                1,
                &data,
            );

            assert_eq!(
                load_pcd(file.path(), |_, _| {}).unwrap().colors,
                vec![expected]
            );
        }
    }

    #[test]
    fn rejects_ascii_packed_f32_rgba_that_cannot_preserve_nan_payload_bits() {
        let file = ascii_fixture(
            "x y z rgba",
            "4 4 4 4",
            "F F F F",
            "1 1 1 1",
            1,
            "1 2 3 NaN\n",
        );

        let error = load_pcd(file.path(), |_, _| {}).unwrap_err().to_string();

        assert!(error.contains("ASCII"), "{error}");
        assert!(error.contains("rgba"), "{error}");
        assert!(error.contains("U32 or I32"), "{error}");
        assert!(error.contains("binary"), "{error}");
    }

    #[test]
    fn preserves_binary_packed_f32_rgba_nan_payload_bits() {
        let packed = u32::from_be_bytes([0xff, 0x92, 0x34, 0x56]);
        assert!(f32::from_bits(packed).is_nan());
        let mut bytes = xyz_bytes(&[[1.0, 2.0, 3.0]]);
        bytes.extend_from_slice(&packed.to_le_bytes());
        let file = binary_fixture("x y z rgba", "4 4 4 4", "F F F F", 1, &bytes);

        let loaded = load_pcd(file.path(), |_, _| {}).unwrap();

        assert_eq!(loaded.colors, vec![[0x92, 0x34, 0x56, 0xff]]);
    }

    #[test]
    fn ignores_padding_fields_when_mapping_schema_indices() {
        let file = ascii_fixture(
            "x _ y z r g b",
            "4 1 4 4 1 1 1",
            "F U F F U U U",
            "1 4 1 1 1 1 1",
            1,
            "1 0 0 0 0 2 3 10 20 30\n",
        );

        let loaded = load_pcd(file.path(), |_, _| {}).unwrap();

        assert_eq!(loaded.positions, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(loaded.colors, vec![[10, 20, 30, 255]]);
    }

    #[test]
    fn applies_color_source_priority_on_colliding_schemas() {
        let rgba = u32::from_be_bytes([40, 10, 20, 30]);
        let rgb = u32::from_be_bytes([0, 50, 60, 70]);
        let cases = [
            (
                "x y z rgba rgb r g b intensity",
                "4 4 4 4 4 1 1 1 4",
                "F F F U U U U U F",
                format!("0 0 5 {rgba} {rgb} 80 90 100 110\n"),
                [10, 20, 30, 40],
            ),
            (
                "x y z rgb r g b intensity",
                "4 4 4 4 1 1 1 4",
                "F F F U U U U F",
                format!("0 0 5 {rgb} 80 90 100 110\n"),
                [50, 60, 70, 255],
            ),
            (
                "x y z r g b intensity",
                "4 4 4 1 1 1 4",
                "F F F U U U F",
                "0 0 5 80 90 100 110\n".to_owned(),
                [80, 90, 100, 255],
            ),
            (
                "x y z intensity",
                "4 4 4 4",
                "F F F F",
                "0 0 5 110\n".to_owned(),
                [128, 128, 128, 255],
            ),
            (
                "x y z",
                "4 4 4",
                "F F F",
                "0 0 5\n".to_owned(),
                [128, 0, 128, 255],
            ),
        ];

        for (fields, sizes, types, data, expected) in cases {
            let counts = fields
                .split_whitespace()
                .map(|_| "1")
                .collect::<Vec<_>>()
                .join(" ");
            let file = ascii_fixture(fields, sizes, types, &counts, 1, &data);

            assert_eq!(
                load_pcd(file.path(), |_, _| {}).unwrap().colors,
                vec![expected],
                "schema {fields} selected the wrong color source"
            );
        }
    }

    #[test]
    fn loads_separate_integer_color_channels_with_optional_alpha() {
        let rgb = ascii_fixture(
            "x y z r g b",
            "4 4 4 1 1 1",
            "F F F U U U",
            "1 1 1 1 1 1",
            1,
            "0 0 0 10 20 30\n",
        );
        let rgba = ascii_fixture(
            "x y z r g b a",
            "4 4 4 2 2 2 2",
            "F F F U U U U",
            "1 1 1 1 1 1 1",
            1,
            "0 0 0 100 110 120 130\n",
        );

        assert_eq!(
            load_pcd(rgb.path(), |_, _| {}).unwrap().colors,
            vec![[10, 20, 30, 255]]
        );
        assert_eq!(
            load_pcd(rgba.path(), |_, _| {}).unwrap().colors,
            vec![[100, 110, 120, 130]]
        );
    }

    #[test]
    fn normalizes_intensity_to_grayscale() {
        let file = ascii_fixture(
            "x y z intensity",
            "4 4 4 2",
            "F F F U",
            "1 1 1 1",
            3,
            "0 0 0 10\n0 0 1 20\n0 0 2 30\n",
        );

        assert_eq!(
            load_pcd(file.path(), |_, _| {}).unwrap().colors,
            vec![[0, 0, 0, 255], [128, 128, 128, 255], [255, 255, 255, 255]]
        );
    }

    #[test]
    fn falls_back_to_normalized_height_colors() {
        let file = ascii_fixture(
            "x y z",
            "4 4 4",
            "F F F",
            "1 1 1",
            3,
            "0 0 -2\n0 0 0\n0 0 2\n",
        );

        assert_eq!(
            load_pcd(file.path(), |_, _| {}).unwrap().colors,
            vec![[0, 0, 255, 255], [128, 0, 128, 255], [255, 0, 0, 255]]
        );
    }

    #[test]
    fn skips_non_finite_coordinates() {
        let file = ascii_fixture(
            "x y z",
            "4 4 4",
            "F F F",
            "1 1 1",
            3,
            "1 2 3\nnan 2 3\n1 inf 3\n",
        );

        let loaded = load_pcd(file.path(), |_, _| {}).unwrap();

        assert_eq!(loaded.positions, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(loaded.colors.len(), 1);
        assert_eq!(loaded.skipped_points, 2);
        assert_eq!(loaded.declared_points, 3);
    }

    #[test]
    fn rejects_missing_xyz_and_reports_discovered_fields() {
        let file = ascii_fixture("x y intensity", "4 4 2", "F F U", "1 1 1", 1, "1 2 3\n");

        let error = load_pcd(file.path(), |_, _| {}).unwrap_err().to_string();

        assert!(error.contains("required fields x, y, z"), "{error}");
        assert!(error.contains("x, y, intensity"), "{error}");
    }

    #[test]
    fn rejects_truncated_binary_record_with_point_index() {
        let mut bytes = xyz_bytes(&[[1.0, 2.0, 3.0]]);
        bytes.extend_from_slice(&4.0_f32.to_le_bytes());
        let file = binary_fixture("x y z", "4 4 4", "F F F", 2, &bytes);

        let error = load_pcd(file.path(), |_, _| {}).unwrap_err().to_string();

        assert!(error.contains("point 1"), "{error}");
    }

    #[test]
    fn rejects_binary_compressed_explicitly() {
        let mut fixture = NamedTempFile::new().unwrap();
        write!(fixture, "VERSION 0.7\nFIELDS x y z\nSIZE 4 4 4\nTYPE F F F\nCOUNT 1 1 1\nWIDTH 1\nHEIGHT 1\nPOINTS 1\nDATA binary_compressed\n").unwrap();

        let error = load_pcd(fixture.path(), |_, _| {}).unwrap_err().to_string();

        assert!(error.contains("binary_compressed"), "{error}");
        assert!(error.contains("unsupported"), "{error}");
    }

    #[test]
    fn returns_error_when_declared_capacity_cannot_be_reserved() {
        let file = ascii_fixture("x y z", "4 4 4", "F F F", "1 1 1", u64::MAX, "");

        let error = load_pcd(file.path(), |_, _| {}).unwrap_err().to_string();

        assert!(error.contains("reserve"), "{error}");
        assert!(error.contains(&u64::MAX.to_string()), "{error}");
    }

    #[test]
    fn allocates_only_buffers_required_by_color_source() {
        let explicit = allocate_buffers(
            8,
            ColorSource::Packed {
                index: 3,
                alpha: true,
            },
        )
        .unwrap();
        assert!(explicit.positions.capacity() >= 8);
        assert!(explicit.colors.capacity() >= 8);
        assert_eq!(explicit.fallback_values.capacity(), 0);

        let fallback = allocate_buffers(8, ColorSource::Height).unwrap();
        assert!(fallback.positions.capacity() >= 8);
        assert_eq!(fallback.colors.capacity(), 0);
        assert!(fallback.fallback_values.capacity() >= 8);
    }
}
