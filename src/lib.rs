#![feature(with_options)]

use bytemuck::cast_slice;
use bytemuck::cast_slice_mut;
use mmarinus::Known;
use std::fs::File;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::unix::prelude::*;
use std::path::Path;
use std::slice::{from_raw_parts, from_raw_parts_mut};

use bytemuck::Pod;
use mmarinus::{perms, sealed::Type, Kind, Map};

pub trait Mmap<'a> {
    fn size(&self) -> usize;
    fn as_slice(&self) -> &'a [u8];
    fn as_slice_mut(&mut self) -> &'a mut [u8];
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

pub struct MmapFile<'a, T: Pod> {
    map: Box<dyn Mmap<'a>>,
    _type: PhantomData<T>,
}

impl<'a, T: Pod> MmapFile<'a, T> {
    pub fn map<U: Known + 'static>(mut file: File, offset: i64) -> Self {
        let len = file.metadata().unwrap().len();
        if len > usize::max_value() as u64 {
            panic!("file too large");
        }

        let map = Box::new(
            Map::map(len as usize)
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

    pub fn open<P: AsRef<Path>>(
        filename: P,
        offset: i64,
    ) -> Result<MmapFile<'a, T>, std::io::Error> {
        let file = File::with_options().read(true).write(true).open(filename)?;

        Ok(MmapFile::map::<perms::ReadWrite>(file, offset))
    }

    pub fn with_capacity<P: AsRef<Path>>(
        filename: P,
        capacity: usize,
    ) -> Result<MmapFile<'a, T>, std::io::Error> {
        let file = File::with_options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(filename)?;

        file.set_len((capacity * core::mem::size_of::<T>()) as u64)?;

        Ok(MmapFile::map::<perms::ReadWrite>(file, 0))
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
    use std::fs::OpenOptions;
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
        file_path.push("test_basic");

        let mut fa = MmapFile::with_capacity(file_path, 4096).unwrap();
        let slice = &mut *fa;
        assert_eq!(slice, &[0u8; 4096]);
        for i in 0..slice.len() {
            slice[i] = 1;
        }
        assert_eq!(slice, &[1u8; 4096]);
    }
}
