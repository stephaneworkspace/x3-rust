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

const CRC_TABLE: [u16; 256] = [
  0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7, 0x8108, 0x9129, 0xa14a, 0xb16b, 0xc18c, 0xd1ad,
  0xe1ce, 0xf1ef, 0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6, 0x9339, 0x8318, 0xb37b, 0xa35a,
  0xd3bd, 0xc39c, 0xf3ff, 0xe3de, 0x2462, 0x3443, 0x0420, 0x1401, 0x64e6, 0x74c7, 0x44a4, 0x5485, 0xa56a, 0xb54b,
  0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d, 0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695, 0x46b4,
  0xb75b, 0xa77a, 0x9719, 0x8738, 0xf7df, 0xe7fe, 0xd79d, 0xc7bc, 0x48c4, 0x58e5, 0x6886, 0x78a7, 0x0840, 0x1861,
  0x2802, 0x3823, 0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b, 0x5af5, 0x4ad4, 0x7ab7, 0x6a96,
  0x1a71, 0x0a50, 0x3a33, 0x2a12, 0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a, 0x6ca6, 0x7c87,
  0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41, 0xedae, 0xfd8f, 0xcdec, 0xddcd, 0xad2a, 0xbd0b, 0x8d68, 0x9d49,
  0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70, 0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a,
  0x9f59, 0x8f78, 0x9188, 0x81a9, 0xb1ca, 0xa1eb, 0xd10c, 0xc12d, 0xf14e, 0xe16f, 0x1080, 0x00a1, 0x30c2, 0x20e3,
  0x5004, 0x4025, 0x7046, 0x6067, 0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e, 0x02b1, 0x1290,
  0x22f3, 0x32d2, 0x4235, 0x5214, 0x6277, 0x7256, 0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
  0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405, 0xa7db, 0xb7fa, 0x8799, 0x97b8, 0xe75f, 0xf77e,
  0xc71d, 0xd73c, 0x26d3, 0x36f2, 0x0691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634, 0xd94c, 0xc96d, 0xf90e, 0xe92f,
  0x99c8, 0x89e9, 0xb98a, 0xa9ab, 0x5844, 0x4865, 0x7806, 0x6827, 0x18c0, 0x08e1, 0x3882, 0x28a3, 0xcb7d, 0xdb5c,
  0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a, 0x4a75, 0x5a54, 0x6a37, 0x7a16, 0x0af1, 0x1ad0, 0x2ab3, 0x3a92,
  0xfd2e, 0xed0f, 0xdd6c, 0xcd4d, 0xbdaa, 0xad8b, 0x9de8, 0x8dc9, 0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83,
  0x1ce0, 0x0cc1, 0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8, 0x6e17, 0x7e36, 0x4e55, 0x5e74,
  0x2e93, 0x3eb2, 0x0ed1, 0x1ef0,
];

pub fn crc16(data: &[u8]) -> u16 {
  let mut crc: u16 = 0xffff; // initial CRC value

  // calculate the CRC over the data bytes
  for d in data {
    let lookup: usize = (d ^ (crc >> 8) as u8) as usize;
    crc = (crc << 8) ^ CRC_TABLE[lookup];
  }

  crc
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
  use crate::crc::crc16;

  #[test]
  fn test_crc() {
    let header: [u8; 20] = [
      0x78, 0x33, // 'x3'
      0x01, 0x01, // Source id
      0x27, 0x10, // n bytes
      0x19, 0xd0, // n samples
      0x00, 0x00, // time
      0x00, 0x00, //   ..
      0x00, 0x00, //   ..
      0x00, 0x00, //   ..
      0xad, 0xdb, // Header crc
      0x6f, 0x61, // Payload crc
    ];

    assert_eq!(0xaddb, crc16(&header[0..16]));

    let payload: [u8; 150] = [
      0xf2, 0x2b, 0xf4, 0x86, 0xb0, 0xe1, 0x6e, 0xca, 0x9a, 0x35, 0x29, 0xa7, 0x51, 0xcd, 0xee, 0xd5, 0xc9, 0x30, 0x94,
      0x21, 0x38, 0xda, 0x56, 0x97, 0x84, 0x44, 0x93, 0xd9, 0x44, 0x60, 0xb4, 0x9c, 0x57, 0x34, 0xd2, 0x1d, 0x2b, 0x69,
      0x11, 0xe9, 0xd6, 0x9a, 0x46, 0xc4, 0x2d, 0xc2, 0x3e, 0x26, 0x25, 0x42, 0xd8, 0xcd, 0xd2, 0xfb, 0x66, 0x6a, 0xe7,
      0x7b, 0xa0, 0x57, 0x8b, 0x20, 0x42, 0xd2, 0x67, 0xf6, 0x67, 0xfa, 0xe5, 0x5a, 0xd6, 0x19, 0x17, 0x19, 0x79, 0xf5,
      0xfc, 0xdb, 0x38, 0xb3, 0x9b, 0x86, 0x5f, 0xcd, 0x2f, 0xa5, 0xf5, 0x3a, 0xcc, 0x62, 0x6e, 0xa3, 0x93, 0xeb, 0x43,
      0xb6, 0x29, 0xaa, 0x62, 0xc5, 0x07, 0xa0, 0xfd, 0x13, 0xdd, 0x40, 0x24, 0x2f, 0x49, 0xc4, 0x85, 0xfa, 0xcf, 0xd2,
      0x83, 0x14, 0x2d, 0x3a, 0x33, 0x0e, 0x4e, 0xf8, 0x11, 0x7a, 0xfc, 0x80, 0x3e, 0xf4, 0x6e, 0x2b, 0x48, 0x63, 0x80,
      0x36, 0xfd, 0x09, 0xec, 0x09, 0x2f, 0x58, 0x36, 0x08, 0x34, 0x0f, 0xb8, 0x1f, 0x60, 0x3f, 0x17, 0xc5,
    ];

    assert_eq!(2073, crc16(&payload));
  }
}
