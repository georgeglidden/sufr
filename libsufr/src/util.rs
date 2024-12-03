use crate::types::{
    FromUsize, Int, SequenceFileData, OUTFILE_VERSION, SENTINEL_CHARACTER,
};
use anyhow::{anyhow, bail, Result};
use needletail::parse_fastx_file;
use regex::Regex;
use std::{fs::File, io::Read, slice};

// --------------------------------------------------
pub fn seed_mask_difference(positions: &[usize]) -> Vec<usize> {
    // Mask: 1001101
    // M: [0, 3, 4, 6] 
    // U: [0, 1, 2, 3]
    // D: [0, 2, 2, 3]
    positions
        .iter()
        .enumerate()
        .map(|(i, &m)| m - i)
        .collect()
}

// --------------------------------------------------
pub fn seed_mask_positions(bytes: &[u8]) -> Vec<usize> {
    // b"101" -> [0, 2]
    bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'1').then_some(i))
        .collect()
}

// --------------------------------------------------
pub fn valid_seed_mask(mask: &str) -> bool {
    let seed_re = Regex::new("^1+0[01]*1$").unwrap();
    seed_re.is_match(mask)
}

// --------------------------------------------------
pub fn vec_to_slice_u8<T>(vec: &[T]) -> &[u8]
where
    T: Int + FromUsize<T> + Sized + Send + Sync + serde::ser::Serialize,
{
    unsafe {
        slice::from_raw_parts(
            vec.as_ptr() as *const _,
            std::mem::size_of_val(vec), //vec.len() * mem::size_of::<T>(),
        )
    }
}

// --------------------------------------------------
pub fn slice_u8_to_vec<T>(buffer: &[u8], len: usize) -> Vec<T>
where
    T: Int + FromUsize<T> + Sized + Send + Sync + serde::ser::Serialize,
{
    unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const _, len).to_vec() }
}

// --------------------------------------------------
// Utility function to find length of the input text
// determine whether to build u32 or u64
pub fn read_text_length(filename: &str) -> Result<usize> {
    let mut file = File::open(filename).map_err(|e| anyhow!("{filename}: {e}"))?;

    // Meta (version, is_dna)
    let mut buffer = [0; 4];
    file.read_exact(&mut buffer)?;

    let outfile_version = buffer[0];
    if outfile_version == OUTFILE_VERSION {
        // Length of text is the next usize
        let mut buffer = [0; 8];
        file.read_exact(&mut buffer)?;
        Ok(usize::from_ne_bytes(buffer))
    } else {
        bail!("Unknown sufr version {outfile_version}");
    }
}

// --------------------------------------------------
// Utility function to read FASTA/Q file for sequence
// data needed by SufrBuilder
pub fn read_sequence_file(
    filename: &str,
    sequence_delimiter: u8,
) -> Result<SequenceFileData> {
    let mut reader = parse_fastx_file(filename)?;
    let mut seq: Vec<u8> = Vec::with_capacity(u32::MAX as usize);
    let mut headers: Vec<String> = vec![];
    let mut start_positions: Vec<usize> = vec![];
    let mut i = 0;
    while let Some(rec) = reader.next() {
        let rec = rec?;
        if i > 0 {
            seq.push(sequence_delimiter);
        }

        // Record current length as start position
        start_positions.push(seq.len());
        let mut tmp: Vec<u8> = rec.seq().iter().copied().collect();
        seq.append(&mut tmp);
        i += 1;

        headers.push(String::from_utf8(rec.id().to_vec())?);
    }

    // File delimiter
    seq.push(SENTINEL_CHARACTER);

    Ok(SequenceFileData {
        seq,
        start_positions,
        headers,
    })
}

// --------------------------------------------------
// Utility function used by SufrBuilder
pub fn usize_to_bytes(value: usize) -> Vec<u8> {
    // Determine the size of usize in bytes
    let size = std::mem::size_of::<usize>();

    // Create a vector to hold the bytes
    let mut bytes = Vec::with_capacity(size);

    // Convert usize to bytes
    for i in 0..size {
        bytes.push((value >> (i * 8)) as u8);
    }

    bytes
}

// --------------------------------------------------
#[cfg(test)]
mod tests {
    use super::{
        read_sequence_file, read_text_length, seed_mask_positions, slice_u8_to_vec,
        usize_to_bytes, valid_seed_mask, vec_to_slice_u8,
        seed_mask_difference
    };
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_seed_mask_positions() -> Result<()> {
        assert_eq!(seed_mask_positions(b"101"), [0, 2]);
        assert_eq!(seed_mask_positions(b"101101"), [0, 2, 3, 5]);
        Ok(())
    }

    #[test]
    fn test_seed_mask_difference() -> Result<()> {
        // Empty is not a failure
        assert_eq!(seed_mask_difference(&[]), []);

        //              0  1
        // "100001" -> [0, 5]
        assert_eq!(seed_mask_difference(&[0, 5]), [0, 4]);
        //               0  1  2  3
        // "1001101" -> [0, 3, 4, 6] 
        assert_eq!(seed_mask_difference(&[0, 3, 4, 6]), [0, 2, 2, 3]);
        Ok(())
    }

    #[test]
    fn test_read_sequence_file() -> Result<()> {
        let file = "../data/inputs/2.fa";
        let sequence_delimiter = b'N';
        let res = read_sequence_file(file, sequence_delimiter);
        assert!(res.is_ok());
        let data = res.unwrap();
        assert_eq!(data.seq, b"ACGTacgtNacgtACGT$");
        assert_eq!(data.start_positions, [0, 9]);
        assert_eq!(data.headers, ["ABC", "DEF"]);
        Ok(())
    }

    #[test]
    fn test_read_text_length() -> Result<()> {
        let sufr_file = "../data/inputs/2.sufr";
        let res = read_text_length(sufr_file);
        assert!(res.is_ok());
        let len = res.unwrap();
        assert_eq!(len, 18);
        Ok(())
    }

    #[test]
    fn test_usize_to_bytes() -> Result<()> {
        assert_eq!(usize_to_bytes(usize::MIN), [0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(1), [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(10), [10, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(100), [100, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(1000), [232, 3, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(10000), [16, 39, 0, 0, 0, 0, 0, 0]);
        assert_eq!(usize_to_bytes(100000), [160, 134, 1, 0, 0, 0, 0, 0]);
        assert_eq!(
            usize_to_bytes(usize::MAX),
            [255, 255, 255, 255, 255, 255, 255, 255]
        );
        Ok(())
    }

    #[test]
    fn test_slice_u8_to_vec() -> Result<()> {
        let res: Vec<u32> = slice_u8_to_vec(&[0, 0, 0, 0], 1);
        assert_eq!(res, &[0u32]);

        let res: Vec<u64> = slice_u8_to_vec(&[0, 0, 0, 0, 0, 0, 0, 0], 1);
        assert_eq!(res, &[0u64]);

        let res: Vec<u32> = slice_u8_to_vec(&[1, 0, 0, 0], 1);
        assert_eq!(res, &[1u32]);

        let res: Vec<u64> = slice_u8_to_vec(&[1, 0, 0, 0, 0, 0, 0, 0], 1);
        assert_eq!(res, &[1u64]);

        let res: Vec<u32> = slice_u8_to_vec(&[255, 255, 255, 255], 1);
        assert_eq!(res, &[u32::MAX]);

        let res: Vec<u64> =
            slice_u8_to_vec(&[255, 255, 255, 255, 255, 255, 255, 255], 1);
        assert_eq!(res, &[u64::MAX]);

        let res: Vec<u32> = slice_u8_to_vec(&[0, 0, 0, 0, 255, 255, 255, 255], 2);
        assert_eq!(res, &[0u32, u32::MAX]);

        let res: Vec<u64> = slice_u8_to_vec(
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            ],
            2,
        );
        assert_eq!(res, &[0u64, u64::MAX]);

        Ok(())
    }

    #[test]
    fn test_vec_to_slice_u8() -> Result<()> {
        let res = vec_to_slice_u8(&[0u32]);
        assert_eq!(res, &[0, 0, 0, 0]);

        let res = vec_to_slice_u8(&[0u64]);
        assert_eq!(res, &[0, 0, 0, 0, 0, 0, 0, 0]);

        let res = vec_to_slice_u8(&[1u32]);
        assert_eq!(res, &[1, 0, 0, 0]);

        let res = vec_to_slice_u8(&[1u64]);
        assert_eq!(res, &[1, 0, 0, 0, 0, 0, 0, 0]);

        let res = vec_to_slice_u8(&[u32::MAX]);
        assert_eq!(res, &[255, 255, 255, 255]);

        let res = vec_to_slice_u8(&[u64::MAX]);
        assert_eq!(res, &[255, 255, 255, 255, 255, 255, 255, 255]);

        let res = vec_to_slice_u8(&[0u32, u32::MAX]);
        assert_eq!(res, &[0, 0, 0, 0, 255, 255, 255, 255]);

        let res = vec_to_slice_u8(&[0u64, u64::MAX]);
        assert_eq!(
            res,
            &[0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255]
        );

        Ok(())
    }

    #[test]
    fn test_valid_seed_mask() -> Result<()> {
        let valid = ["101", "1001", "1101", "10101", "1110110110100001"];
        let invalid = [
            "", "abc", "1", "11", "111", "0", "00", "0111", "11100", "1a01",
        ];

        for pattern in valid {
            assert!(valid_seed_mask(pattern));
        }

        for pattern in invalid {
            assert!(!valid_seed_mask(pattern));
        }

        Ok(())
    }
}
