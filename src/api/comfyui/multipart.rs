use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use super::uploads::Upload;
use crate::api::{ApiError, ApiResult};

pub(super) struct MultipartBody {
    parts: VecDeque<Part>,
    pub(super) content_length: u64,
    pub(super) content_type: String,
}

impl MultipartBody {
    pub(super) fn new(upload: &Upload, boundary: &str) -> ApiResult<Self> {
        let mut file = upload.snapshot.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let current_len = file.metadata()?.len();
        if current_len != upload.byte_len {
            return Err(ApiError::InvalidRequest(format!(
                "upload snapshot length changed after hashing: expected {}, found {current_len}",
                upload.byte_len
            )));
        }
        let mut parts = VecDeque::new();
        push_bytes(
            &mut parts,
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                upload.name
            )
            .into_bytes(),
        );
        parts.push_back(Part::File(file));
        push_bytes(&mut parts, b"\r\n".to_vec());
        push_field(
            &mut parts,
            boundary,
            "overwrite",
            &upload.overwrite.to_string(),
        );
        push_field(&mut parts, boundary, "type", &upload.upload_type);
        push_field(&mut parts, boundary, "subfolder", &upload.subfolder);
        push_bytes(&mut parts, format!("--{boundary}--\r\n").into_bytes());
        let non_file_len = parts
            .iter()
            .map(|part| match part {
                Part::Bytes(cursor) => cursor.get_ref().len() as u64,
                Part::File(_) => 0,
            })
            .sum::<u64>();
        let content_length = non_file_len.checked_add(upload.byte_len).ok_or_else(|| {
            ApiError::InvalidRequest("multipart content length overflow".to_owned())
        })?;
        Ok(Self {
            parts,
            content_length,
            content_type: format!("multipart/form-data; boundary={boundary}"),
        })
    }
}

impl Read for MultipartBody {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let Some(part) = self.parts.front_mut() else {
                return Ok(0);
            };
            let count = match part {
                Part::Bytes(cursor) => cursor.read(buffer)?,
                Part::File(file) => file.read(buffer)?,
            };
            if count != 0 {
                return Ok(count);
            }
            self.parts.pop_front();
        }
    }
}

enum Part {
    Bytes(Cursor<Vec<u8>>),
    File(File),
}

fn push_field(parts: &mut VecDeque<Part>, boundary: &str, name: &str, value: &str) {
    push_bytes(
        parts,
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .into_bytes(),
    );
}

fn push_bytes(parts: &mut VecDeque<Part>, bytes: Vec<u8>) {
    parts.push_back(Part::Bytes(Cursor::new(bytes)));
}
