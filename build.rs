//! Embed the icon and native-control manifest without an SDK/resource compiler.
//! Both GNU ld and MSVC link accept the same single-section COFF object.
use std::{env, fs, path::PathBuf};

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn alloc(bytes: &mut Vec<u8>, count: usize) -> usize {
    let at = bytes.len();
    bytes.resize(at + count, 0);
    at
}
fn directory(bytes: &mut Vec<u8>, count: usize) -> usize {
    let at = alloc(bytes, 16 + count * 8);
    put16(bytes, at + 14, count as u16);
    at
}
fn entry(bytes: &mut [u8], directory: usize, index: usize, id: u32, offset: usize, subdir: bool) {
    put32(bytes, directory + 16 + index * 8, id);
    put32(
        bytes,
        directory + 20 + index * 8,
        offset as u32 | if subdir { 0x8000_0000 } else { 0 },
    );
}

fn main() {
    println!("cargo:rerun-if-changed=assets/branding/horizon.ico");
    println!("cargo:rerun-if-changed=assets/horizon.manifest");
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "windows" {
        return;
    }
    let ico = fs::read("assets/branding/horizon.ico").expect("committed Horizon icon");
    assert_eq!(&ico[..4], &[0, 0, 1, 0]);
    let count = u16_at(&ico, 4) as usize;
    let mut group = ico[..6].to_vec();
    let mut icons = Vec::with_capacity(count);
    for index in 0..count {
        let at = 6 + index * 16;
        let len = u32_at(&ico, at + 8) as usize;
        let offset = u32_at(&ico, at + 12) as usize;
        icons.push(ico[offset..offset + len].to_vec());
        group.extend_from_slice(&ico[at..at + 12]);
        group.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }
    let mut resource = Vec::new();
    let root = directory(&mut resource, 3);
    let icon_dir = directory(&mut resource, count);
    let group_dir = directory(&mut resource, 1);
    let manifest_dir = directory(&mut resource, 1);
    entry(&mut resource, root, 0, 3, icon_dir, true); // RT_ICON
    entry(&mut resource, root, 1, 14, group_dir, true); // RT_GROUP_ICON
    entry(&mut resource, root, 2, 24, manifest_dir, true); // RT_MANIFEST
    let mut data_entries = Vec::new();
    for (index, parent) in (0..count)
        .map(|i| (i, icon_dir))
        .chain(std::iter::once((0, group_dir)))
        .chain(std::iter::once((0, manifest_dir)))
    {
        let language = directory(&mut resource, 1);
        entry(
            &mut resource,
            parent,
            index,
            (index + 1) as u32,
            language,
            true,
        );
        let data = alloc(&mut resource, 16);
        entry(
            &mut resource,
            language,
            0,
            if parent == manifest_dir { 0 } else { 1033 },
            data,
            false,
        );
        data_entries.push(data);
    }
    let manifest = fs::read("assets/horizon.manifest").expect("committed common-controls manifest");
    for (data, image) in data_entries.iter().zip(
        icons
            .iter()
            .chain(std::iter::once(&group))
            .chain(std::iter::once(&manifest)),
    ) {
        while resource.len() % 4 != 0 {
            resource.push(0);
        }
        let start = resource.len();
        put32(&mut resource, *data, start as u32);
        put32(&mut resource, *data + 4, image.len() as u32);
        resource.extend_from_slice(image);
    }
    let (machine, relocation): (u16, u16) =
        match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "x86_64" => (0x8664, 3), // IMAGE_REL_AMD64_ADDR32NB
            "aarch64" => (0xaa64, 2),
            "x86" => (0x014c, 7),
            arch => panic!("Unsupported Windows resource architecture: {arch}"),
        };
    let mut object = vec![0; 60]; // COFF header + one section header
    put16(&mut object, 0, machine);
    put16(&mut object, 2, 1);
    let relocations = 60 + resource.len();
    let symbols = relocations + data_entries.len() * 10;
    put32(&mut object, 8, symbols as u32);
    put32(&mut object, 12, 1);
    object[20..25].copy_from_slice(b".rsrc");
    put32(&mut object, 36, resource.len() as u32);
    put32(&mut object, 40, 60);
    put32(&mut object, 44, relocations as u32);
    put16(&mut object, 52, data_entries.len() as u16);
    put32(&mut object, 56, 0x4030_0040); // read-only initialized data, 4-byte alignment
    object.extend_from_slice(&resource);
    for offset in data_entries {
        object.extend_from_slice(&(offset as u32).to_le_bytes());
        object.extend_from_slice(&0_u32.to_le_bytes()); // section symbol
        object.extend_from_slice(&relocation.to_le_bytes());
    }
    let symbol = alloc(&mut object, 18);
    object[symbol..symbol + 5].copy_from_slice(b".rsrc");
    put16(&mut object, symbol + 12, 1);
    object[symbol + 16] = 3; // static section symbol
    object.extend_from_slice(&4_u32.to_le_bytes()); // empty COFF string table
    let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("horizon-icon.o");
    fs::write(&path, object).expect("write icon resource object");
    println!("cargo:rustc-link-arg-bin=horizon={}", path.display());
}
