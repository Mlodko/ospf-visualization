use nom::IResult;

use crate::{gui::modules::dock::packet::parse::lsa::{header::LsaHeader, payload::LsaPayload}, network::node::OspfAdvertisement};

#[derive(Debug, Clone)]
pub struct Lsa {
    pub header: LsaHeader,
    pub payload: LsaPayload,
    pub raw: Vec<u8>,
}

impl PartialEq for Lsa {
    fn eq(&self, other: &Self) -> bool {
        self.header.ls_type     == other.header.ls_type &&
        self.header.ls_id       == other.header.ls_id &&
        self.header.adv_router  == other.header.adv_router
    }
}

impl Lsa {
    /// Parse an LSA from a full LSA byte slice (header + payload).
    /// Returns the remaining input after the LSA, and the constructed `Lsa`.
    pub fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        // Parse header (20 bytes total for OSPFv2 LSAs)
        let (rest_after_header, header) = LsaHeader::from_bytes(input)?;

        // Compute payload start offset (absolute within the packet slice)
        // The header parser consumed `input.len() - rest_after_header.len()` bytes.
        let consumed = input.len() - rest_after_header.len();
        let payload_start_offset = consumed;

        // The header's `length` field is total LSA length (header + payload).
        // Validate we have at least that many bytes in `input`.
        let total_len = header.length.value as usize;
        if input.len() < total_len {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::LengthValue,
            )));
        }

        // Slice out the payload bytes according to total length minus header consumed.
        let payload_len = total_len - consumed;
        let payload_input = &rest_after_header[..payload_len];

        // Parse payload based on ls_type, using absolute payload start offset.
        let (_rest_after_payload, payload) =
            LsaPayload::parse(payload_input, payload_start_offset, header.ls_type.value)?;

        // After parsing the payload, `rest_after_payload` should be empty for a well-formed LSA.
        // Return any remaining bytes from the original input beyond the LSA.
        let remaining_after_lsa = &rest_after_header[payload_len..];
        let raw = input.to_vec();
        
        Ok((remaining_after_lsa, Lsa { header, payload, raw }))
    }
}
#[cfg(test)]
mod tests {
    use super::Lsa;
    use hex::decode;

    #[test]
    fn parse_lsa_dump_all_lines() {
        let data = include_str!("resources/lsa_dump");
        let mut count = 0usize;
        for (idx, line) in data.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let hex_str: String = trimmed.split_whitespace().collect();
            if hex_str.is_empty() {
                continue;
            }
            let bytes = match decode(&hex_str) {
                Ok(b) => b,
                Err(e) => panic!("Hex decode failed on line {}: {} ({})", idx + 1, trimmed, e),
            };
            let (rest, lsa) =
                Lsa::parse(&bytes).expect(&format!("Parse failed on line {}", idx + 1));
            dbg!(&lsa);
            assert!(
                rest.is_empty(),
                "Non-empty remainder on line {}: {} bytes",
                idx + 1,
                rest.len()
            );
            assert_eq!(
                lsa.header.length.value as usize,
                bytes.len(),
                "Header length mismatch on line {}",
                idx + 1
            );
            count += 1;
        }
        assert!(count > 0, "No LSAs parsed");
    }
}
