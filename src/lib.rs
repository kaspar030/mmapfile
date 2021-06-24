use mmarinus::{perms, sealed::Type, Kind, Map};
use std::fs::File;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::unix::prelude::*;
use std::slice::{from_raw_parts, from_raw_parts_mut};

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

pub struct MmapFile<'a> {
    map: Box<dyn Mmap<'a>>,
}

impl<'a> MmapFile<'a> {
    pub fn map(file: &'a mut File, offset: i64) -> Self {
        let len = file.metadata().unwrap().len();
        if len > usize::max_value() as u64 {
            panic!("file too large");
        }

        let map = Box::new(
            Map::map(len as usize)
                .anywhere()
                .from(file, offset)
                .known::<perms::ReadWrite>(Kind::Private)
                .unwrap(),
        );

        MmapFile { map }
    }
    pub fn size(&self) -> usize {
        self.map.size()
    }
    pub fn as_slice(&self) -> &'a [u8] {
        self.map.as_slice()
    }
    pub fn as_slice_mut(&mut self) -> &'a mut [u8] {
        self.map.as_slice_mut()
    }
}

impl<'a> Deref for MmapFile<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> DerefMut for MmapFile<'a> {
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
        println!("{:?}", file_path);
        let mut f = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(file_path.clone())
            .unwrap();

        f.set_len(4096).unwrap();
        println!("file: {:?}", f);

        let mut fa = MmapFile::map(&mut f, 0);
        let slice = &mut *fa;
        assert_eq!(slice, &[0u8; 4096]);
    }

    #[test]
    fn as_slice_mut() {
        let temp = TempDir::default();
        let mut file_path = PathBuf::from(temp.as_ref());
        file_path.push("test_basic");
        println!("{:?}", file_path);
        let mut f = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(file_path.clone())
            .unwrap();
        f.set_len(4096).unwrap();
        println!("file: {:?}", f);

        let mut fa = MmapFile::map(&mut f, 0);
        let slice = &mut *fa;
        assert_eq!(slice, &[0u8; 4096]);
        for i in 0..slice.len() {
            slice[i] = 1;
        }
        assert_eq!(slice, &[1u8; 4096]);
    }
}
