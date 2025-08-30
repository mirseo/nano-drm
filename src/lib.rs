use image::{ImageBuffer, Luma, Rgba};
use lopdf::{
    content::{Content, Operation},
    Dictionary, Document, Object, ObjectId, Stream,
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use reed_solomon_erasure::{galois_8, Error as RSError, ReedSolomon};
use std::fs;
use std::io::Cursor;

// ... (DrmError, From impls, FileType, detect_file_type are unchanged) ...
#[derive(Debug)]
enum DrmError {
    Io(std::io::Error),
    Py(PyErr),
    Rs(RSError),
    Image(image::ImageError),
    Pdf(lopdf::Error),
    Message(String),
}

impl From<std::io::Error> for DrmError {
    fn from(err: std::io::Error) -> DrmError {
        DrmError::Io(err)
    }
}
impl From<PyErr> for DrmError {
    fn from(err: PyErr) -> DrmError {
        DrmError::Py(err)
    }
}
impl From<RSError> for DrmError {
    fn from(err: RSError) -> DrmError {
        DrmError::Rs(err)
    }
}
impl From<image::ImageError> for DrmError {
    fn from(err: image::ImageError) -> DrmError {
        DrmError::Image(err)
    }
}
impl From<lopdf::Error> for DrmError {
    fn from(err: lopdf::Error) -> DrmError {
        DrmError::Pdf(err)
    }
}

impl From<DrmError> for PyErr {
    fn from(err: DrmError) -> PyErr {
        match err {
            DrmError::Io(e) => PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()),
            DrmError::Py(e) => e,
            DrmError::Rs(e) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Reed-Solomon error: {:?}",
                e
            )),
            DrmError::Image(e) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Image processing error: {}",
                e
            )),
            DrmError::Pdf(e) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "PDF processing error: {}",
                e
            )),
            DrmError::Message(s) => PyErr::new::<pyo3::exceptions::PyValueError, _>(s),
        }
    }
}

enum FileType {
    Png,
    Pdf,
    Unsupported,
}

fn detect_file_type(data: &[u8]) -> FileType {
    if data.len() > 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        FileType::Png
    } else if data.len() > 4 && &data[0..4] == b"%PDF" {
        FileType::Pdf
    } else {
        FileType::Unsupported
    }
}

fn embed_in_png(image_data: &[u8], data_to_embed: &[u8]) -> Result<Vec<u8>, DrmError> {
    let mut img = image::load_from_memory(image_data)?.to_rgba8();
    let capacity = img.as_raw().len();
    let required_capacity = data_to_embed.len() * 8;

    if capacity < required_capacity {
        return Err(DrmError::Message(format!(
            "Not enough space in image. Required: {} bits, Available: {} bits",
            required_capacity, capacity
        )));
    }

    let mut bit_iter = data_to_embed
        .iter()
        .flat_map(|byte| (0..8).map(move |i| (byte >> i) & 1));
    let img_buffer = img.as_mut();

    for byte_chunk in img_buffer.iter_mut() {
        if let Some(bit) = bit_iter.next() {
            *byte_chunk = (*byte_chunk & 0xFE) | bit;
        } else {
            break;
        }
    }

    let mut result_bytes = Vec::new();
    img.write_to(
        &mut Cursor::new(&mut result_bytes),
        image::ImageOutputFormat::Png,
    )?;

    Ok(result_bytes)
}

fn extract_from_png(image_data: &[u8]) -> Result<Vec<u8>, DrmError> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = image::load_from_memory(image_data)?.to_rgba8();
    let mut bit_iter = img.as_raw().iter().map(|byte| byte & 1);

    let mut len_bytes = [0u8; 8];
    for i in 0..8 {
        for j in 0..8 {
            if let Some(bit) = bit_iter.next() {
                len_bytes[i] |= bit << j;
            } else {
                return Err(DrmError::Message(
                    "Reached end of image before reading payload length".to_string(),
                ));
            }
        }
    }
    let payload_len = u64::from_be_bytes(len_bytes) as usize;

    let mut payload = vec![0u8; payload_len];
    for i in 0..payload_len {
        for j in 0..8 {
            if let Some(bit) = bit_iter.next() {
                payload[i] |= bit << j;
            } else {
                return Err(DrmError::Message(
                    "Reached end of image before reading full payload".to_string(),
                ));
            }
        }
    }

    Ok(payload)
}

fn extract_from_pdf(pdf_data: &[u8]) -> Result<Vec<u8>, DrmError> {
    let doc = Document::load_mem(pdf_data)?;
    let page_ids = doc.get_pages().values().cloned().collect::<Vec<ObjectId>>();
    for page_id in page_ids {
        let page_dict = doc.get_object(page_id)?.as_dict()?;
        if let Ok(&Object::Reference(res_id)) = page_dict.get(b"Resources") {
            let resources = doc.get_object(res_id)?.as_dict()?;
            if let Ok(&Object::Reference(xobj_id)) = resources.get(b"XObject") {
                let xobjects = doc.get_object(xobj_id)?.as_dict()?;
                if let Ok(&Object::Reference(img_id)) = xobjects.get(b"UpdrmImg") {
                    let stream = doc.get_object(img_id)?.as_stream()?;
                    let img_bytes = stream.content.clone();
                    let img = image::load_from_memory(&img_bytes)?.to_luma8();
                    let raw = img.into_raw();
                    if raw.len() < 8 {
                        return Err(DrmError::Message("Embedded data too short".to_string()));
                    }
                    let mut len_bytes = [0u8; 8];
                    len_bytes.copy_from_slice(&raw[0..8]);
                    let payload_len = u64::from_be_bytes(len_bytes) as usize;
                    if raw.len() < 8 + payload_len {
                        return Err(DrmError::Message(
                            "Embedded data length mismatch".to_string(),
                        ));
                    }
                    return Ok(raw[8..8 + payload_len].to_vec());
                }
            }
        }
    }
    Err(DrmError::Message(
        "No embedded data found in PDF".to_string(),
    ))
}

fn embed_in_pdf(pdf_data: &[u8], data_to_embed: &[u8]) -> Result<Vec<u8>, DrmError> {
    let mut doc = Document::load_mem(pdf_data)?;

    // 1. Create noise image
    let side = (data_to_embed.len() as f64).sqrt().ceil() as u32;
    let noise_img_buffer: ImageBuffer<Luma<u8>, _> = ImageBuffer::from_fn(side, side, |x, y| {
        let index = (y * side + x) as usize;
        Luma([if index < data_to_embed.len() {
            data_to_embed[index]
        } else {
            0
        }])
    });
    let mut noise_png_bytes = vec![];
    noise_img_buffer.write_to(
        &mut Cursor::new(&mut noise_png_bytes),
        image::ImageOutputFormat::Png,
    )?;

    // 2. Create Image XObject
    let image_xobject = Stream::new(
        Dictionary::from_iter(vec![
            (b"Type".to_vec(), Object::Name(b"XObject".to_vec())),
            (b"Subtype".to_vec(), Object::Name(b"Image".to_vec())),
            (b"Width".to_vec(), Object::Integer(side as i64)),
            (b"Height".to_vec(), Object::Integer(side as i64)),
            (b"ColorSpace".to_vec(), Object::Name(b"DeviceGray".to_vec())),
            (b"BitsPerComponent".to_vec(), Object::Integer(8)),
            (b"Filter".to_vec(), Object::Name(b"FlateDecode".to_vec())),
        ]),
        noise_png_bytes,
    );
    let image_id = doc.add_object(image_xobject);

    // 3. Create Graphics State dictionary for transparency
    let gs_dict = Dictionary::from_iter(vec![(b"ca".to_vec(), Object::Real(0.01))]);
    let gs_id = doc.add_object(gs_dict);

    // 4. Iterate over pages and add the image
    let page_ids = doc.get_pages().values().cloned().collect::<Vec<ObjectId>>();
    for page_id in page_ids {
        let resources_id = {
            let page_dict = doc.get_object(page_id)?.as_dict()?;
            match page_dict.get(b"Resources") {
                Ok(Object::Reference(id)) => *id,
                _ => {
                    let new_res_id = doc.add_object(Dictionary::new());
                    doc.get_object_mut(page_id)?
                        .as_dict_mut()?
                        .set(b"Resources", new_res_id);
                    new_res_id
                }
            }
        };

        let xobject_id = if let Ok(&Object::Reference(id)) =
            doc.get_object(resources_id)?.as_dict()?.get(b"XObject")
        {
            id
        } else {
            let new_id = doc.add_object(Dictionary::new());
            doc.get_object_mut(resources_id)?
                .as_dict_mut()?
                .set(b"XObject", new_id);
            new_id
        };
        let gstate_id = if let Ok(&Object::Reference(id)) =
            doc.get_object(resources_id)?.as_dict()?.get(b"ExtGState")
        {
            id
        } else {
            let new_id = doc.add_object(Dictionary::new());
            doc.get_object_mut(resources_id)?
                .as_dict_mut()?
                .set(b"ExtGState", new_id);
            new_id
        };
        doc.get_object_mut(xobject_id)?
            .as_dict_mut()?
            .set(b"UpdrmImg", image_id);
        doc.get_object_mut(gstate_id)?
            .as_dict_mut()?
            .set(b"UpdrmGS", gs_id);

        let content_ops = vec![
            Operation::new("q", vec![]),
            Operation::new("gs", vec![Object::Name(b"UpdrmGS".to_vec())]),
            Operation::new(
                "cm",
                vec![
                    10.0.into(),
                    0.into(),
                    0.into(),
                    10.0.into(),
                    50.into(),
                    50.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"UpdrmImg".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        let new_content_stream = Stream::new(
            Dictionary::new(),
            Content {
                operations: content_ops,
            }
            .encode()?,
        );
        let new_content_id = doc.add_object(new_content_stream);

        let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
        let mut contents_array = match page_dict.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![Object::Reference(*id)],
            Ok(Object::Array(arr)) => arr.clone(),
            _ => vec![],
        };
        contents_array.push(Object::Reference(new_content_id));
        page_dict.set(b"Contents", Object::Array(contents_array));
    }

    let mut result_bytes = Vec::new();
    doc.save_to(&mut result_bytes)?;
    Ok(result_bytes)
}

// ... (write and read functions are unchanged) ...
#[pyfunction]
fn write(py: Python, file_path: String, data: &Bound<'_, PyAny>) -> Result<(), DrmError> {
    let _ = py;

    let raw_data: Vec<u8> = if let Ok(text) = data.extract::<String>() {
        text.into_bytes()
    } else if let Ok(bytes) = data.extract::<Vec<u8>>() {
        bytes
    } else {
        return Err(DrmError::Py(
            PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "Data must be a string (JSON) or bytes (image)".to_string(),
            ),
        ));
    };

    let len_bytes = (raw_data.len() as u32).to_be_bytes();
    let mut full_data = len_bytes.to_vec();
    full_data.extend_from_slice(&raw_data);

    const DATA_SHARDS: usize = 10;
    const PARITY_SHARDS: usize = 4;
    let rs = ReedSolomon::<galois_8::Field>::new(DATA_SHARDS, PARITY_SHARDS)?;
    let shard_len = (full_data.len() + DATA_SHARDS - 1) / DATA_SHARDS;
    let mut shards: Vec<Vec<u8>> = full_data.chunks(shard_len).map(|c| c.to_vec()).collect();
    shards.iter_mut().for_each(|s| s.resize(shard_len, 0));
    for _ in 0..PARITY_SHARDS {
        shards.push(vec![0; shard_len]);
    }
    rs.encode(&mut shards)?;
    let encoded_data: Vec<u8> = shards.into_iter().flatten().collect();

    let mut final_payload = (encoded_data.len() as u64).to_be_bytes().to_vec();
    final_payload.extend_from_slice(&encoded_data);

    let original_file_bytes = fs::read(&file_path)?;

    let file_type = detect_file_type(&original_file_bytes);

    let modified_file_bytes = match file_type {
        FileType::Png => embed_in_png(&original_file_bytes, &final_payload)?,
        FileType::Pdf => embed_in_pdf(&original_file_bytes, &final_payload)?,
        FileType::Unsupported => {
            return Err(DrmError::Message(
                "Unsupported file type. Only PNG and PDF are supported.".to_string(),
            ))
        }
    };

    fs::write(&file_path, &modified_file_bytes)?;

    Ok(())
}

#[pyfunction]
fn read(py: Python, file_path: String) -> Result<PyObject, DrmError> {
    let file_bytes = fs::read(&file_path)?;
    let file_type = detect_file_type(&file_bytes);

    let encoded_data = match file_type {
        FileType::Png => extract_from_png(&file_bytes)?,
        FileType::Pdf => extract_from_pdf(&file_bytes)?,
        FileType::Unsupported => {
            return Err(DrmError::Message(
                "Unsupported file type for extraction.".to_string(),
            ))
        }
    };

    const DATA_SHARDS: usize = 10;
    const PARITY_SHARDS: usize = 4;
    let rs = ReedSolomon::<galois_8::Field>::new(DATA_SHARDS, PARITY_SHARDS)?;
    let shard_len = encoded_data.len() / (DATA_SHARDS + PARITY_SHARDS);
    let mut shards: Vec<Option<Vec<u8>>> = encoded_data
        .chunks(shard_len)
        .map(|c| Some(c.to_vec()))
        .collect();

    rs.reconstruct(&mut shards)?;

    let full_data: Vec<u8> = shards
        .into_iter()
        .take(DATA_SHARDS)
        .filter_map(|s| s)
        .flatten()
        .collect();

    if full_data.len() < 4 {
        return Err(DrmError::Message(
            "Reconstructed data is too short to contain length header.".to_string(),
        ));
    }
    let len_bytes: [u8; 4] = full_data[0..4]
        .try_into()
        .map_err(|_| DrmError::Message("Failed to read data length from payload".to_string()))?;
    let raw_data_len = u32::from_be_bytes(len_bytes) as usize;

    if full_data.len() < 4 + raw_data_len {
        return Err(DrmError::Message(
            "Reconstructed data is shorter than specified by its length header.".to_string(),
        ));
    }
    let raw_data = &full_data[4..(4 + raw_data_len)];

    Ok(PyBytes::new_bound(py, raw_data).into())
}

#[pymodule]
fn mirseo_updrm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(write, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    Ok(())
}
