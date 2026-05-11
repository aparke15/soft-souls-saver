use camino::Utf8PathBuf;
use sha2::{Digest, Sha256};
use std::{fs, io::Read};
use walkdir::WalkDir;
use zip::{ZipWriter, write::FileOptions};

pub fn zip_dir(src: &Utf8PathBuf, dst_zip: &Utf8PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(
        dst_zip
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent for zip path"))?,
    )?;

    let file = fs::File::create(dst_zip)?;
    let mut zip = ZipWriter::new(file);
    let opts = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let base = src.clone();

    for entry in WalkDir::new(&base).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        let rel = path.strip_prefix(base.as_std_path())?;

        if entry.file_type().is_dir() {
            let name = rel.to_string_lossy();
            if !name.is_empty() {
                zip.add_directory(name, opts)?;
            }
        } else if entry.file_type().is_file() {
            zip.start_file(rel.to_string_lossy(), opts)?;
            let mut f = fs::File::open(path)?;
            std::io::copy(&mut f, &mut zip)?;
        }
    }

    zip.finish()?;
    Ok(())
}

pub fn sha256_file(p: &Utf8PathBuf) -> anyhow::Result<String> {
    let mut f = fs::File::open(p)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}
