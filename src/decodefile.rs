/**************************************************************************
 *                                                                        *
 * Rust implementation of the X3 lossless audio compression protocol.     *
 *                                                                        *
 * Copyright (C) 2019 Simon M. Werner <simonwerner@gmail.com>             *
 *                                                                        *
 * This program is free software; you can redistribute it and/or modify   *
 * it under the terms of the GNU General Public License as published by   *
 * the Free Software Foundation, either version 3 of the License, or      *
 * (at your option) any later version.                                    *
 *                                                                        *
 * This program is distributed in the hope that it will be useful,        *
 * but WITHOUT ANY WARRANTY; without even the implied warranty of         *
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the           *
 * GNU General Public License for more details.                           *
 *                                                                        *
 * You should have received a copy of the GNU General Public License      *
 * along with this program. If not, see <http://www.gnu.org/licenses/>.   *
 *                                                                        *
 **************************************************************************/

// std
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path;

// externs
use crate::hound;

// this crate
use crate::decoder;
use crate::error;
use crate::{crc, x3};

use crate::x3::{FrameHeader, X3aSpec};
use error::X3Error;
use quick_xml::events::Event;
use quick_xml::Reader;

pub const X3_READ_BUFFER_SIZE: usize = 1024 * 24;
pub const X3_WRITE_BUFFER_SIZE: usize = X3_READ_BUFFER_SIZE * 8;

pub struct X3aReader {
  reader: BufReader<File>,
  spec: X3aSpec,
  remaing_bytes: usize,
  read_buf: [u8; X3_READ_BUFFER_SIZE],

  /// The count of errors.
  /// TODO: Count each type of error
  frame_errors: usize,
}

impl X3aReader {
  pub fn open<P: AsRef<path::Path>>(filename: P) -> Result<Self, X3Error> {
    let file = File::open(filename).unwrap();
    let mut remaing_bytes = file.metadata()?.len() as usize;
    let mut reader = BufReader::with_capacity(64 * 1024, file);

    let (spec, header_size) = read_archive_header(&mut reader)?;
    remaing_bytes -= header_size;

    Ok(Self {
      reader,
      spec,
      remaing_bytes,
      read_buf: [0u8; X3_READ_BUFFER_SIZE],
      frame_errors: 0,
    })
  }

  pub fn spec(&self) -> &X3aSpec {
    &self.spec
  }

  fn read_bytes(&mut self, mut buf_len: usize) -> std::io::Result<()> {
    if self.remaing_bytes < buf_len {
      buf_len = self.remaing_bytes;
    }
    self.remaing_bytes -= buf_len;
    self.reader.read_exact(&mut self.read_buf[0..buf_len])
  }

  fn read_frame_header(&mut self) -> Result<FrameHeader, X3Error> {
    self.read_bytes(x3::FrameHeader::LENGTH)?;
    decoder::read_frame_header(&self.read_buf[0..x3::FrameHeader::LENGTH])
  }

  fn read_frame_payload(&mut self, header: &FrameHeader) -> Result<(), X3Error> {
    self.read_bytes(header.payload_len)?;

    let payload = &self.read_buf[0..header.payload_len];
    let crc = crc::crc16(&payload);
    if crc != header.payload_crc {
      return Err(X3Error::FrameHeaderInvalidPayloadCRC);
    }

    Ok(())
  }

  pub fn decode_next_frame(&mut self, wav_buf: &mut [i16; X3_WRITE_BUFFER_SIZE]) -> Result<Option<usize>, X3Error> {
    // We have reached the end of the file
    if self.remaing_bytes <= x3::FrameHeader::LENGTH {
      return Ok(None);
    }

    // Get the header details
    let frame_header = self.read_frame_header()?;
    let samples = frame_header.samples as usize;
    if self.remaing_bytes < frame_header.payload_len {
      return Ok(None);
    }

    if frame_header.payload_len > X3_READ_BUFFER_SIZE {
      // Payload is larger than the available buffer size
      return Err(X3Error::FrameHeaderInvalidPayloadLen);
    }

    // Get the Payload
    self.read_frame_payload(&frame_header)?;
    let x3_bytes = &mut self.read_buf[0..frame_header.payload_len];

    // Do the decoding
    match decoder::decode_frame(x3_bytes, wav_buf, &self.spec.params, samples) {
      Ok(result) => Ok(result),
      Err(err) => {
        self.frame_errors += 1;
        println!("Frame error: {:?}", err);
        Ok(None)
      }
    }
  }
}

///
/// Read the <Archive Header> from in the input buffer.
///
fn read_archive_header(reader: &mut BufReader<File>) -> Result<(X3aSpec, usize), X3Error> {
  // <Archive Id>
  {
    let mut arc_header = [0u8; x3::Archive::ID.len()];
    reader.read_exact(&mut arc_header)?;
    if !arc_header.eq(x3::Archive::ID) {
      return Err(X3Error::ArchiveHeaderXMLInvalidKey);
    }
  }

  // <XML MetaData>
  let header = {
    let mut header_buf = [0u8; x3::FrameHeader::LENGTH];
    reader.read_exact(&mut header_buf)?;
    decoder::read_frame_header(&mut header_buf)?
  };

  // Get the payload
  let mut payload: Vec<u8> = vec![0; header.payload_len];
  reader.read_exact(&mut payload)?;
  let xml = String::from_utf8_lossy(&payload);

  let (sample_rate, params) = parse_xml(&xml)?;

  let header_size = x3::FrameHeader::LENGTH + payload.len();

  Ok((
    X3aSpec {
      sample_rate,
      params,
      channels: header.channels,
    },
    header_size,
  ))
}

///
/// Convert an .x3a (X3 Archive) file to a .wav file.  
///
/// Note: the x3a can contain some meta data of the recording that may be lost, such as the time
///       of the recording and surplus XML payload data that has been embedded into the X3A header.
///
/// ### Arguments
///
/// * `x3a_filename` - the input X3A file to decode.
/// * `wav_filename` - the output wav file to write to.  It will be overwritten.
///
pub fn x3a_to_wav<P: AsRef<path::Path>>(x3a_filename: P, wav_filename: P) -> Result<(), X3Error> {
  let mut x3a_reader = X3aReader::open(x3a_filename)?;

  let x3_spec = x3a_reader.spec();
  let spec = hound::WavSpec {
    channels: 1, //x3_spec.channels as u16,
    sample_rate: x3_spec.sample_rate,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
  };

  let mut writer = hound::WavWriter::create(wav_filename, spec)?;
  let mut wav = [0i16; X3_WRITE_BUFFER_SIZE];
  loop {
    match x3a_reader.decode_next_frame(&mut wav)? {
      Some(samples) => {
        write_samples(&mut writer, &wav, samples)?;
      }
      None => break,
    }
  }

  Ok(())
}

fn write_samples(
  writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
  buf: &[i16],
  num_samples: usize,
) -> Result<(), X3Error> {
  let mut fast_writer = writer.get_i16_writer(num_samples as u32);
  for i in 0..num_samples {
    unsafe {
      fast_writer.write_sample_unchecked(buf[i]);
    }
  }
  fast_writer.flush()?;
  Ok(())
}

///
/// Parse the XML header that contains the parameters for the wav output.
///
fn parse_xml(xml: &str) -> Result<(u32, x3::Parameters), X3Error> {
  let mut reader = Reader::from_str(xml);
  reader.trim_text(true);

  let mut buf = Vec::new();
  let mut fs = Vec::with_capacity(3);
  let mut bl = Vec::with_capacity(3);
  let mut codes = Vec::with_capacity(3);
  let mut th = Vec::with_capacity(3);

  // The `Reader` does not implement `Iterator` because it outputs borrowed data (`Cow`s)
  loop {
    match reader.read_event(&mut buf) {
      Ok(Event::Start(ref e)) => match e.name() {
        b"FS" => fs.push(reader.read_text(e.name(), &mut Vec::new()).unwrap()),
        b"BLKLEN" => bl.push(reader.read_text(e.name(), &mut Vec::new()).unwrap()),
        b"CODES" => codes.push(reader.read_text(e.name(), &mut Vec::new()).unwrap()),
        b"T" => th.push(reader.read_text(e.name(), &mut Vec::new()).unwrap()),
        _ => (),
      },
      Ok(Event::Eof) => break, // exits the loop when reaching end of file
      Err(e) => {
        println!(
          "Error reading X3 Archive header (XML) at position {}: {:?}",
          reader.buffer_position(),
          e
        );
        return Err(X3Error::ArchiveHeaderXMLInvalid);
      }
      _ => (), // There are several other `Event`s we do not consider here
    }

    // if we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
    buf.clear();
  }
  println!("sample rate: {}", fs[0]);
  println!("block length: {}", bl[0]);
  println!("Rice codes: {}", codes[0]);
  println!("thresholds: {}", th[0]);

  let sample_rate = fs[0].parse::<u32>().unwrap();
  let block_len = bl[0].parse::<u32>().unwrap();
  let mut rice_code_ids = Vec::new();
  for word in codes[0].split(',') {
    match word {
      "RICE0" => rice_code_ids.push(0),
      "RICE1" => rice_code_ids.push(1),
      "RICE2" => rice_code_ids.push(2),
      "RICE3" => rice_code_ids.push(3),
      "BFP" => (),
      _ => return Err(X3Error::ArchiveHeaderXMLRiceCode),
    };
  }
  let thresholds: Vec<usize> = th[0].split(',').map(|s| s.parse::<usize>().unwrap()).collect();

  let mut rc_array: [usize; 3] = [0; 3];
  let mut th_array: [usize; 3] = [0; 3];

  #[allow(clippy::manual_memcpy)]
  for i in 0..3 {
    rc_array[i] = rice_code_ids[i];
    th_array[i] = thresholds[i];
  }
  let params = x3::Parameters::new(
    block_len as usize,
    x3::Parameters::DEFAULT_BLOCKS_PER_FRAME,
    rc_array,
    th_array,
  )?;

  Ok((sample_rate, params))
}

//
//
//            #######
//               #       ######     ####     #####     ####
//               #       #         #           #      #
//               #       #####      ####       #       ####
//               #       #              #      #           #
//               #       #         #    #      #      #    #
//               #       ######     ####       #       ####
//
//

#[cfg(test)]
mod tests {
  // use crate::decodefile::x3a_to_wav;

  // #[test]
  // fn test_decode_x3a_file() {
  //   x3a_to_wav("~/tmp/test.x3a", "~/tmp/test.wav").unwrap();
  // }
}
