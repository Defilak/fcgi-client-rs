// Copyright 2022 jmjoy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Internal FastCGI protocol metadata structures and parsing.
//!
//! This module contains the internal structures and constants used
//! for parsing and generating FastCGI protocol messages.

use crate::{
    error::{ClientError, ClientResult},
    Params,
};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::{
    borrow::Cow,
    cmp::min,
    collections::HashMap,
    fmt::{self, Debug, Display},
    mem::size_of,
    ops::{Deref, DerefMut},
};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// FastCGI protocol version 1
pub(crate) const VERSION_1: u8 = 1;
/// Maximum length for FastCGI content
pub(crate) const MAX_LENGTH: usize = 0xffff;
/// Length of FastCGI header in bytes
pub(crate) const HEADER_LEN: usize = size_of::<Header>();

/// FastCGI request types as defined in the protocol specification.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RequestType {
    /// Begin request record type
    BeginRequest = 1,
    /// Abort request record type
    AbortRequest = 2,
    /// End request record type
    EndRequest = 3,
    /// Parameters record type
    Params = 4,
    /// Stdin record type
    Stdin = 5,
    /// Stdout record type
    Stdout = 6,
    /// Stderr record type
    Stderr = 7,
    /// Data record type
    Data = 8,
    /// Get values record type
    GetValues = 9,
    /// Get values result record type
    GetValuesResult = 10,
    /// Unknown type record type
    UnknownType = 11,
}

impl RequestType {
    /// Converts a u8 value to RequestType.
    ///
    /// # Arguments
    ///
    /// * `u` - The numeric value to convert
    fn from_u8(u: u8) -> Self {
        match u {
            1 => RequestType::BeginRequest,
            2 => RequestType::AbortRequest,
            3 => RequestType::EndRequest,
            4 => RequestType::Params,
            5 => RequestType::Stdin,
            6 => RequestType::Stdout,
            7 => RequestType::Stderr,
            8 => RequestType::Data,
            9 => RequestType::GetValues,
            10 => RequestType::GetValuesResult,
            _ => RequestType::UnknownType,
        }
    }
}

impl Display for RequestType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", *self as u8)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Header {
    /// FastCGI protocol version
    pub(crate) version: u8,
    /// Type of the FastCGI record
    pub(crate) r#type: RequestType,
    /// Request ID for this record
    pub(crate) request_id: u16,
    /// Length of the content data
    pub(crate) content_length: u16,
    /// Length of padding data
    pub(crate) padding_length: u8,
    /// Reserved byte
    pub(crate) reserved: u8,
}

impl Header {
    /// Writes data to a stream in batches with proper FastCGI headers.
    ///
    /// # Arguments
    ///
    /// * `r#type` - The type of FastCGI record
    /// * `request_id` - The request ID
    /// * `writer` - The writer to write to
    /// * `content` - The content to write
    /// * `before_write` - Optional callback to modify header before writing
    pub(crate) async fn write_to_stream_batches<F, R, W>(
        r#type: RequestType, request_id: u16, writer: &mut W, content: &mut R,
        before_write: Option<F>,
    ) -> io::Result<()>
    where
        F: Fn(Header) -> Header,
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut buf = vec![0u8; MAX_LENGTH];
        let mut had_written = false;

        loop {
            let read = content.read(&mut buf).await?;
            if had_written && read == 0 {
                break;
            }

            let buf = &buf[..read];
            let mut header = Self::new(r#type.clone(), request_id, buf);
            if let Some(ref f) = before_write {
                header = f(header);
            }
            header.write_to_stream(writer, buf).await?;

            had_written = true;
        }
        Ok(())
    }

    /// Creates a new header with given parameters.
    ///
    /// # Arguments
    ///
    /// * `r#type` - The type of FastCGI record
    /// * `request_id` - The request ID
    /// * `content` - The content data
    fn new(r#type: RequestType, request_id: u16, content: &[u8]) -> Self {
        let content_length = min(content.len(), MAX_LENGTH) as u16;
        Self {
            version: VERSION_1,
            r#type,
            request_id,
            content_length,
            padding_length: (-(content_length as i16) & 7) as u8,
            reserved: 0,
        }
    }

    /// Writes the header and content to a stream.
    ///
    /// # Arguments
    ///
    /// * `writer` - The writer to write to
    /// * `content` - The content to write
    async fn write_to_stream<W: AsyncWrite + Unpin>(
        self, writer: &mut W, content: &[u8],
    ) -> io::Result<()> {
        let mut buf: Bytes = (&self).into();

        writer.write_all_buf(&mut buf).await?;
        writer.write_all(content).await?;

        if self.padding_length > 0 {
            let padding = [0u8; 7]; // Max padding is 7 bytes
            writer
                .write_all(&padding[..self.padding_length as usize])
                .await?;
        }
        Ok(())
    }

    /// Creates a new header by reading from a stream.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to read from
    pub(crate) async fn new_from_stream<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Self> {
        let mut buf = BytesMut::zeroed(HEADER_LEN);
        reader.read_exact(&mut buf).await?;
        Ok(Self::from(buf))
    }

    /// Reads content from a stream based on the header's content length.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to read from
    pub(crate) async fn read_content_from_stream<R: AsyncRead + Unpin>(
        &self, reader: &mut R,
    ) -> io::Result<BytesMut> {
        let mut buf = BytesMut::zeroed(self.content_length as usize);
        reader.read_exact(&mut buf).await?;
        let mut padding_buf = BytesMut::zeroed(self.padding_length as usize);
        reader.read_exact(&mut padding_buf).await?;
        Ok(buf)
    }
}

impl Into<Bytes> for &Header {
    fn into(self) -> Bytes {
        let mut buf = BytesMut::with_capacity(HEADER_LEN);
        buf.put_u8(self.version);
        buf.put_u8(self.r#type as u8);
        buf.put_u16(self.request_id);
        buf.put_u16(self.content_length);
        buf.put_u8(self.padding_length);
        buf.put_u8(self.reserved);
        buf.freeze()
    }
}

impl From<BytesMut> for Header {
    /// Creates a new header from a buffer.
    ///
    /// # Arguments
    ///
    /// * `buf` - The buffer containing header data
    fn from(mut buf: BytesMut) -> Self {
        Self {
            version: buf.get_u8(),
            r#type: RequestType::from_u8(buf.get_u8()),
            request_id: buf.get_u16(),
            content_length: buf.get_u16(),
            padding_length: buf.get_u8(),
            reserved: buf.get_u8(),
        }
    }
}

/// FastCGI application roles.
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
pub enum Role {
    /// Responder role - handles requests and returns responses
    Responder = 1,
    /// Authorizer role - performs authorization checks
    Authorizer = 2,
    /// Filter role - filters data between web server and application
    Filter = 3,
}

/// Begin request record body data.
#[derive(Debug)]
pub(crate) struct BeginRequest {
    /// The role of the application
    pub(crate) role: Role,
    /// Flags byte (bit 0 = keep alive flag)
    pub(crate) flags: u8,
    /// Reserved bytes
    pub(crate) reserved: [u8; 5],
}

impl BeginRequest {
    /// Creates a new begin request record.
    ///
    /// # Arguments
    ///
    /// * `role` - The role of the application
    /// * `keep_alive` - Whether to keep the connection alive
    pub(crate) fn new(role: Role, keep_alive: bool) -> Self {
        Self {
            role,
            flags: keep_alive as u8,
            reserved: [0; 5],
        }
    }

    /// Converts the begin request to bytes.
    pub(crate) fn to_content(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(8);
        buf.put_u16(self.role as u16);
        buf.put_u8(self.flags);
        buf.put_slice(&self.reserved);
        buf
    }
}

/// Complete begin request record with header and content.
#[derive(Debug)]
pub(crate) struct BeginRequestRec {
    /// The FastCGI header
    pub(crate) header: Header,
    /// The begin request data
    pub(crate) begin_request: BeginRequest,
    /// The serialized content
    pub(crate) content: BytesMut,
}

impl BeginRequestRec {
    /// Creates a new begin request record.
    ///
    /// # Arguments
    ///
    /// * `request_id` - The request ID
    /// * `role` - The role of the application
    /// * `keep_alive` - Whether to keep the connection alive
    pub(crate) fn new(request_id: u16, role: Role, keep_alive: bool) -> Self {
        let begin_request = BeginRequest::new(role, keep_alive);
        let content = begin_request.to_content();
        let header = Header::new(RequestType::BeginRequest, request_id, &content);
        Self {
            header,
            begin_request,
            content,
        }
    }

    /// Writes the begin request record to a stream.
    ///
    /// # Arguments
    ///
    /// * `writer` - The writer to write to
    pub(crate) async fn write_to_stream<W: AsyncWrite + Unpin>(
        self, writer: &mut W,
    ) -> io::Result<()> {
        self.header.write_to_stream(writer, &self.content).await
    }
}

/// Parameter length encoding for FastCGI.
#[derive(Debug, Clone, Copy)]
pub enum ParamLength {
    /// Short length (0-127 bytes)
    Short(u8),
    /// Long length (128+ bytes)
    Long(u32),
}

impl ParamLength {
    /// Creates a new parameter length encoding.
    ///
    /// # Arguments
    ///
    /// * `length` - The length to encode
    pub fn new(length: usize) -> Self {
        if length < 128 {
            ParamLength::Short(length as u8)
        } else {
            let mut length = length;
            length |= 1 << 31;
            ParamLength::Long(length as u32)
        }
    }

    /// Converts the parameter length to bytes.
    pub fn content(self) -> BytesMut {
        match self {
            ParamLength::Short(l) => {
                let mut buf = BytesMut::with_capacity(1);
                buf.put_u8(l);
                buf
            }
            ParamLength::Long(l) => {
                let mut buf = BytesMut::with_capacity(4);
                buf.put_u32(l);
                buf
            }
        }
    }
}

/// A single parameter name-value pair.
#[derive(Debug)]
pub struct ParamPair<'a> {
    /// Length of the parameter name
    name_length: ParamLength,
    /// Length of the parameter value
    value_length: ParamLength,
    /// The parameter name
    name_data: Cow<'a, str>,
    /// The parameter value
    value_data: Cow<'a, str>,
}

impl<'a> ParamPair<'a> {
    /// Creates a new parameter pair.
    ///
    /// # Arguments
    ///
    /// * `name` - The parameter name
    /// * `value` - The parameter value
    fn new(name: Cow<'a, str>, value: Cow<'a, str>) -> Self {
        let name_length = ParamLength::new(name.len());
        let value_length = ParamLength::new(value.len());
        Self {
            name_length,
            value_length,
            name_data: name,
            value_data: value,
        }
    }

    /// Writes the parameter pair to a buffer.
    ///
    /// # Arguments
    ///
    /// * `buf` - The buffer to write to
    fn write_to_buf(&self, buf: &mut BytesMut) {
        let name_len = self.name_length.content();
        buf.extend_from_slice(&name_len);
        let value_len = self.value_length.content();
        buf.extend_from_slice(&value_len);
        buf.extend_from_slice(self.name_data.as_bytes());
        buf.extend_from_slice(self.value_data.as_bytes());
    }
}

/// Collection of parameter pairs.
#[derive(Debug)]
pub(crate) struct ParamPairs<'a>(Vec<ParamPair<'a>>);

impl<'a> ParamPairs<'a> {
    /// Creates parameter pairs from a Params object.
    ///
    /// # Arguments
    ///
    /// * `params` - The parameters to convert
    pub(crate) fn new(params: Params<'a>) -> Self {
        let mut param_pairs = Vec::new();
        let params: HashMap<Cow<'a, str>, Cow<'a, str>> = params.into();
        for (name, value) in params.into_iter() {
            let param_pair = ParamPair::new(name, value);
            param_pairs.push(param_pair);
        }

        Self(param_pairs)
    }

    /// Converts the parameter pairs to bytes.
    pub(crate) fn to_content(&self) -> Bytes {
        let mut buf = BytesMut::new();

        for param_pair in self.iter() {
            param_pair.write_to_buf(&mut buf);
        }

        buf.freeze()
    }
}

impl<'a> Deref for ParamPairs<'a> {
    type Target = Vec<ParamPair<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for ParamPairs<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// FastCGI protocol status codes.
#[derive(Debug)]
#[repr(u8)]
pub enum ProtocolStatus {
    /// Request completed successfully
    RequestComplete = 0,
    /// This app can't multiplex connections
    CantMpxConn = 1,
    /// New request rejected; too busy
    Overloaded = 2,
    /// Role value not known
    UnknownRole = 3,
}

impl ProtocolStatus {
    /// Converts a u8 value to ProtocolStatus.
    ///
    /// # Arguments
    ///
    /// * `u` - The numeric value to convert
    pub fn from_u8(u: u8) -> Self {
        match u {
            0 => ProtocolStatus::RequestComplete,
            1 => ProtocolStatus::CantMpxConn,
            2 => ProtocolStatus::Overloaded,
            _ => ProtocolStatus::UnknownRole,
        }
    }

    /// Converts the protocol status to a client result.
    ///
    /// # Arguments
    ///
    /// * `app_status` - The application status code
    pub(crate) fn convert_to_client_result(self, app_status: u32) -> ClientResult<()> {
        match self {
            ProtocolStatus::RequestComplete => Ok(()),
            _ => Err(ClientError::new_end_request_with_protocol_status(
                self, app_status,
            )),
        }
    }
}

/// End request record body data.
#[derive(Debug)]
pub struct EndRequest {
    /// The application status code
    pub(crate) app_status: u32,
    /// The protocol status
    pub(crate) protocol_status: ProtocolStatus,
    /// Reserved bytes
    #[allow(dead_code)]
    reserved: [u8; 3],
}

impl From<BytesMut> for EndRequest {
    fn from(mut buf: BytesMut) -> Self {
        let app_status = buf.get_u32();
        let protocol_status = ProtocolStatus::from_u8(buf.get_u8());
        let mut reserved = [0u8; 3];
        buf.copy_to_slice(&mut reserved);

        Self {
            app_status,
            protocol_status,
            reserved,
        }
    }
}

/// Complete end request record with header and content.
#[derive(Debug)]
pub(crate) struct EndRequestRec {
    /// The FastCGI header
    #[allow(dead_code)]
    header: Header,
    /// The end request data
    pub(crate) end_request: EndRequest,
}

impl EndRequestRec {
    /// Creates an end request record from a header and reader.
    ///
    /// # Arguments
    ///
    /// * `header` - The FastCGI header
    /// * `reader` - The reader to read content from
    pub(crate) async fn from_header<R: AsyncRead + Unpin>(
        header: &Header, reader: &mut R,
    ) -> io::Result<Self> {
        let header = header.clone();
        let content = header.read_content_from_stream(reader).await?;
        Ok(Self::new_from_buf(header, content))
    }

    /// Creates an end request record from a header and buffer.
    ///
    /// # Arguments
    ///
    /// * `header` - The FastCGI header
    /// * `buf` - The buffer containing the end request data
    pub(crate) fn new_from_buf(header: Header, buf: BytesMut) -> Self {
        Self {
            header,
            end_request: EndRequest::from(buf),
        }
    }
}
