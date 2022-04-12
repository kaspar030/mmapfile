use std::fs::{File, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::unix::prelude::*;
use std::path::Path;
use std::slice::{from_raw_parts, from_raw_parts_mut};

use bincode::Options;
use bytemuck::cast_slice;
use bytemuck::cast_slice_mut;
use bytemuck::Pod;
use mmarinus::Known;
use mmarinus::{perms, sealed::Type, Kind, Map};
use serde::{Deserialize, Serialize};

pub trait Mmap<'a> {
    fn size(&self) -> usize;
    fn as_slice(&self) -> &'a [u8];
    fn as_slice_mut(&mut self) -> &'a mut [u8];
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MmapFileHdr {
    magic: [u8; 4],
    typename: String,
    size: u64,
}

impl MmapFileHdr {
    fn new<T: Pod>(size: u64) -> Self {
        Self {
            magic: *b"MMAP",
            typename: std::any::type_name::<T>().into(),
            size,
        }
    }

    fn serialized_size(&self) -> u64 {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialized_size(self)
            .unwrap()
    }

    fn padded_size<T: Pod>(&self) -> u64 {
        let elem_align = core::mem::align_of::<T>();

        const fn align_up(val: u64, alignment: u64) -> u64 {
            ((val + alignment - 1) / alignment) * alignment
        }

        let own_size = self.serialized_size();
        if elem_align > 1 {
            println!(
                "aligning me={} elem={} to {}",
                own_size,
                elem_align,
                align_up(own_size as u64, elem_align as u64)
            );
            align_up(own_size as u64, elem_align as u64)
        } else {
            own_size
        }
    }

    pub fn serialize_into<W>(&self, writer: W) -> Result<(), Box<bincode::ErrorKind>>
    where
        W: Write,
    {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize_into(writer, self)
    }

    pub fn deserialize_from<R>(reader: R) -> Result<Self, Box<bincode::ErrorKind>>
    where
        R: Read,
    {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .deserialize_from::<R, Self>(reader)
    }
}

impl<'a, T: Type> Mmap<'a> for Map<T> {
    fn size(&self) -> usize {
        self.size()
    }

    fn as_slice(&self) -> &'a [u8] {
        unsafe { from_raw_parts(self.addr() as *const u8, self.size()) }
    }

    fn as_slice_mut(&mut self) -> &'a mut [u8] {
        unsafe { from_raw_parts_mut(self.addr() as *mut u8, self.size()) }
    }
}

const fn page_align(val: u64) -> u64 {
    ((val + 4095) / 4096) * 4096
}

pub struct MmapFile<'a, T: Pod> {
    map: Box<dyn Mmap<'a>>,
    _type: PhantomData<T>,
}

impl<'a, T: Pod> MmapFile<'a, T> {
    pub fn map<U: Known + 'static>(mut file: File, offset: i64, capacity: usize) -> Self {
        let len = file.metadata().unwrap().len();
        let data_len = capacity * std::mem::size_of::<T>();
        if len > usize::max_value() as u64 {
            panic!("file too large");
        }

        println!("offset:{} data_len: {} file_len:{}", offset, data_len, len);

        if len < data_len as u64 + (offset as u64) {
            panic!("file too small");
        }

        let map = Box::new(
            Map::map(data_len)
                .anywhere()
                .from(&mut file, offset)
                .known::<U>(Kind::Shared)
                .unwrap(),
        );

        MmapFile {
            map,
            _type: PhantomData,
        }
    }

    pub fn open<P: AsRef<Path>>(filename: P) -> Result<MmapFile<'a, T>, std::io::Error> {
        let file = OpenOptions::new().read(true).write(true).open(filename)?;
        let hdr = MmapFileHdr::deserialize_from(&file).unwrap();
        if hdr.typename != std::any::type_name::<T>() {
            panic!("type mismatch");
        }

        let offset = page_align(MmapFileHdr::serialized_size(&hdr));

        Ok(MmapFile::map::<perms::ReadWrite>(
            file,
            offset as i64,
            hdr.size as usize,
        ))
    }

    pub fn with_capacity<P: AsRef<Path>>(
        filename: P,
        capacity: usize,
    ) -> Result<MmapFile<'a, T>, std::io::Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(filename)?;

        let hdr = MmapFileHdr::new::<T>(capacity as u64);
        hdr.serialize_into(&file).unwrap();
        let hdr_len = page_align(hdr.serialized_size());
        let data_len = (capacity * core::mem::size_of::<T>()) as u64;

        file.set_len(page_align(hdr_len + data_len))?;

        Ok(MmapFile::map::<perms::ReadWrite>(
            file,
            hdr_len as i64,
            capacity as usize,
        ))
    }

    pub fn size(&self) -> usize {
        self.map.size()
    }

    pub fn as_slice(&self) -> &'a [T] {
        cast_slice::<u8, T>(self.map.as_slice())
    }

    pub fn as_slice_mut(&mut self) -> &'a mut [T] {
        cast_slice_mut::<u8, T>(self.map.as_slice_mut())
    }
}

impl<'a, T: Pod> Deref for MmapFile<'a, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a, T: Pod> DerefMut for MmapFile<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_slice_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::MmapFile;
    use crate::MmapFileHdr;
    use std::path::PathBuf;
    use temp_testdir::TempDir;

    #[test]
    fn as_slice() {
        let temp = TempDir::default();
        let mut file_path = PathBuf::from(temp.as_ref());
        file_path.push("test_basic");

        let slice: &[u8] = &mut *MmapFile::with_capacity(file_path, 4096).unwrap();
        assert_eq!(slice, &[0u8; 4096]);
    }

    #[test]
    fn as_slice_mut() {
        let temp = TempDir::default();
        let mut file_path = PathBuf::from(temp.as_ref());
        file_path.push("test_basic_mut");

        let mut fa = MmapFile::with_capacity(file_path, 4096).unwrap();
        let slice = &mut *fa;
        assert_eq!(slice, &[0u8; 4096]);
        for i in 0..slice.len() {
            slice[i] = 1;
        }
        assert_eq!(slice, &[1u8; 4096]);
    }

    #[test]
    fn as_slice_mut_unaligned() {
        let temp = TempDir::default();
        let mut file_path = PathBuf::from(temp.as_ref());
        file_path.push("test_slice_mut_unaligned");

        let mut fa = MmapFile::with_capacity(file_path, 4000).unwrap();
        let slice = &mut *fa;
        assert_eq!(slice.len(), 4000);
        assert_eq!(slice, &[0u8; 4000]);
        for i in 0..slice.len() {
            slice[i] = 1;
        }
        assert_eq!(slice, &[1u8; 4000]);
    }

    #[test]
    fn mmapfilehdr_basic() {
        let hdr: MmapFileHdr = MmapFileHdr::new::<u8>(12345);
        assert_eq!(22, hdr.serialized_size());
        let hdr: MmapFileHdr = MmapFileHdr::new::<u8>(1);
        assert_eq!(22, hdr.serialized_size());
        let hdr: MmapFileHdr = MmapFileHdr::new::<u8>(1 << 32);
        assert_eq!(22, hdr.serialized_size());
    }
}
