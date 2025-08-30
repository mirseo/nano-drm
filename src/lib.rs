use pyo3::prelude::*;
use pyo3::types::PyBytes;
use reed_solomon_erasure::{galois_8, ReedSolomon, Error as RSError};
use image::{ImageBuffer, Rgba};
use std::io::Cursor;
use std::fs;

// ... (DrmError enum and From implementations - unchanged) ...
#[derive(Debug)]
enum DrmError {
    Io(std::io::Error),
    Py(PyErr),
    Rs(RSError),
    Image(image::ImageError),
    Message(String),
}

impl From<std::io::Error> for DrmError { fn from(err: std::io::Error) -> DrmError { DrmError::Io(err) } }
impl From<PyErr> for DrmError { fn from(err: PyErr) -> DrmError { DrmError::Py(err) } }
impl From<RSError> for DrmError { fn from(err: RSError) -> DrmError { DrmError::Rs(err) } }
impl From<image::ImageError> for DrmError { fn from(err: image::ImageError) -> DrmError { DrmError::Image(err) } }

impl From<DrmError> for PyErr {
    fn from(err: DrmError) -> PyErr {
        match err {
            DrmError::Io(e) => PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()),
            DrmError::Py(e) => e,
            DrmError::Rs(e) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Reed-Solomon error: {:?}", e)),
            DrmError::Image(e) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Image processing error: {}", e)),
            DrmError::Message(s) => PyErr::new::<pyo3::exceptions::PyValueError, _>(s),
        }
    }
}

// ... (embed_in_png and extract_from_png are internal helpers, names unchanged) ...
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

    let mut bit_iter = data_to_embed.iter().flat_map(|byte| (0..8).map(move |i| (byte >> i) & 1));
    let img_buffer = img.as_mut();

    for byte_chunk in img_buffer.iter_mut() {
        if let Some(bit) = bit_iter.next() {
            *byte_chunk = (*byte_chunk & 0xFE) | bit;
        } else {
            break;
        }
    }

    let mut result_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut result_bytes), image::ImageOutputFormat::Png)?;
    
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
                return Err(DrmError::Message("Reached end of image before reading payload length".to_string()));
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
                return Err(DrmError::Message("Reached end of image before reading full payload".to_string()));
            }
        }
    }

    Ok(payload)
}


/// Writes data into a file, overwriting it.
#[pyfunction]
fn write(py: Python, file_path: String, data: &Bound<'_, PyAny>) -> Result<(), DrmError> {
    let _ = py;

    // Automatically detect if data is string or bytes
    let raw_data: Vec<u8> = if let Ok(text) = data.extract::<String>() {
        text.into_bytes()
    } else if let Ok(bytes) = data.extract::<Vec<u8>>() {
        bytes
    } else {
        return Err(DrmError::Py(PyErr::new::<pyo3::exceptions::PyTypeError, _>("Data must be a string (JSON) or bytes (image)".to_string())));
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
    for _ in 0..PARITY_SHARDS { shards.push(vec![0; shard_len]); }
    rs.encode(&mut shards)?;
    let encoded_data: Vec<u8> = shards.into_iter().flatten().collect();

    let mut final_payload = (encoded_data.len() as u64).to_be_bytes().to_vec();
    final_payload.extend_from_slice(&encoded_data);

    let original_file_bytes = fs::read(&file_path)?;
    
    let modified_file_bytes = if file_path.to_lowercase().ends_with(".png") {
        embed_in_png(&original_file_bytes, &final_payload)?
    } else {
        return Err(DrmError::Message("Unsupported file type. Only .png is supported.".to_string()));
    };

    // Overwrite the original file
    fs::write(&file_path, &modified_file_bytes)?;

    Ok(())
}

/// Reads data from a file.
#[pyfunction]
fn read(py: Python, file_path: String) -> Result<PyObject, DrmError> {
    let file_bytes = fs::read(&file_path)?;

    let encoded_data = if file_path.to_lowercase().ends_with(".png") {
        extract_from_png(&file_bytes)?
    } else {
        return Err(DrmError::Message("Unsupported file type for extraction.".to_string()));
    };

    const DATA_SHARDS: usize = 10;
    const PARITY_SHARDS: usize = 4;
    let rs = ReedSolomon::<galois_8::Field>::new(DATA_SHARDS, PARITY_SHARDS)?;
    let shard_len = encoded_data.len() / (DATA_SHARDS + PARITY_SHARDS);
    let mut shards: Vec<Option<Vec<u8>>> = encoded_data.chunks(shard_len).map(|c| Some(c.to_vec())).collect();

    rs.reconstruct(&mut shards)?;

    let full_data: Vec<u8> = shards.into_iter().take(DATA_SHARDS).filter_map(|s| s).flatten().collect();

    if full_data.len() < 4 {
        return Err(DrmError::Message("Reconstructed data is too short to contain length header.".to_string()));
    }
    let len_bytes: [u8; 4] = full_data[0..4].try_into().map_err(|_| DrmError::Message("Failed to read data length from payload".to_string()))?;
    let raw_data_len = u32::from_be_bytes(len_bytes) as usize;

    if full_data.len() < 4 + raw_data_len {
        return Err(DrmError::Message("Reconstructed data is shorter than specified by its length header.".to_string()));
    }
    let raw_data = &full_data[4..(4 + raw_data_len)];

    Ok(PyBytes::new_bound(py, raw_data).into())
}


/// A Python module for nano-drm implemented in Rust.
#[pymodule]
fn mirseo_updrm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(write, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    Ok(())
}