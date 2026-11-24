//! Índice persistente de miniaturas por carpeta: `index.bin` (entradas) + `data.bin` (JPEGs).
//!
//! `data.bin` es append-only; `index.bin` se reescribe de forma atómica (temporal + rename).
//! Una entrada vale mientras coincidan tamaño y mtime del archivo original.
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::scoring::Score;

const MAGIC: &[u8; 8] = b"IVTHUMB2";
const COMPACT_MIN_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub mtime_ns: u64,
}

impl FileStamp {
    pub fn of(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        let mtime_ns = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Ok(Self {
            size: meta.len(),
            mtime_ns,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    stamp: FileStamp,
    offset: u64,
    len: u32,
    score: Score,
}

#[derive(Debug)]
pub struct ThumbIndex {
    dir: PathBuf,
    thumb_max: u32,
    scoring_version: u32,
    generation: u32,
    entries: HashMap<String, Entry>,
    data: File,
    data_len: u64,
    unsaved: usize,
}

fn data_path(dir: &Path, generation: u32) -> PathBuf {
    dir.join(format!("data-{generation}.bin"))
}

fn open_data(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

impl ThumbIndex {
    /// Abre (o crea) el índice de `dir`. Un índice ilegible o de otro tamaño de miniatura
    /// se descarta y se empieza vacío: el caché siempre se puede regenerar.
    pub fn open(dir: &Path, thumb_max: u32, scoring_version: u32) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let parsed = fs::read(dir.join("index.bin"))
            .ok()
            .and_then(|bytes| parse_index(&bytes, thumb_max, scoring_version));
        let (generation, mut entries) = parsed.unwrap_or_default();

        let data_file = data_path(dir, generation);
        let data = open_data(&data_file)?;
        if entries.is_empty() {
            data.set_len(0)?;
        }
        let data_len = data.metadata()?.len();
        entries.retain(|_, e| e.offset + e.len as u64 <= data_len);
        remove_other_generations(dir, generation);

        Ok(Self {
            dir: dir.to_path_buf(),
            thumb_max,
            scoring_version,
            generation,
            entries,
            data,
            data_len,
            unsaved: 0,
        })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Hay una entrada para `name`, sin validar tamaño ni mtime (se valida al leer).
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// (offset, len, nota) si la entrada coincide con tamaño y mtime.
    pub fn lookup(&self, name: &str, stamp: FileStamp) -> Option<(u64, u32, Score)> {
        self.entries
            .get(name)
            .filter(|e| e.stamp == stamp)
            .map(|e| (e.offset, e.len, e.score))
    }

    /// Nota guardada para `name`, sin validar tamaño ni mtime.
    pub fn score_of(&self, name: &str) -> Option<Score> {
        self.entries.get(name).map(|e| e.score)
    }

    pub fn read(&self, offset: u64, len: u32) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        read_exact_at(&self.data, &mut buf, offset)?;
        Ok(buf)
    }

    pub fn insert(
        &mut self,
        name: &str,
        stamp: FileStamp,
        bytes: &[u8],
        score: Score,
    ) -> io::Result<()> {
        let len = u32::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "thumbnail too large"))?;
        let offset = self.data.seek(SeekFrom::End(0))?;
        self.data.write_all(bytes)?;
        self.data_len = offset + len as u64;
        self.entries.insert(
            name.to_string(),
            Entry {
                stamp,
                offset,
                len,
                score,
            },
        );
        self.unsaved += 1;
        Ok(())
    }

    pub fn unsaved(&self) -> usize {
        self.unsaved
    }

    #[cfg(test)]
    pub fn data_len(&self) -> u64 {
        self.data_len
    }

    /// Persiste el índice (temporal + rename). Si más de la mitad de `data` es basura,
    /// copia los blobs vivos a una generación nueva y cambia a ella con el mismo rename.
    pub fn save(&mut self) -> io::Result<()> {
        let live: u64 = self.entries.values().map(|e| e.len as u64).sum();
        let compact = self.data_len > COMPACT_MIN_BYTES && live * 2 < self.data_len;

        let (generation, entries, data) = if compact {
            let generation = self.generation.wrapping_add(1);
            let path = data_path(&self.dir, generation);
            let mut out = open_data(&path)?;
            out.set_len(0)?;
            let mut sorted: Vec<_> = self.entries.iter().collect();
            sorted.sort_by_key(|(_, e)| e.offset);
            let mut entries = HashMap::with_capacity(sorted.len());
            let mut offset = 0u64;
            for (name, e) in sorted {
                out.write_all(&self.read(e.offset, e.len)?)?;
                entries.insert(name.clone(), Entry { offset, ..*e });
                offset += e.len as u64;
            }
            out.sync_all()?;
            (generation, entries, Some((out, offset)))
        } else {
            (self.generation, self.entries.clone(), None)
        };

        let tmp = self.dir.join(format!("index.{}.tmp", std::process::id()));
        {
            let mut file = File::create(&tmp)?;
            file.write_all(&encode_index(
                self.thumb_max,
                self.scoring_version,
                generation,
                &entries,
            ))?;
            file.sync_all()?;
        }
        fs::rename(&tmp, self.dir.join("index.bin"))?;

        if let Some((out, len)) = data {
            self.data = out;
            self.data_len = len;
            self.entries = entries;
            self.generation = generation;
            remove_other_generations(&self.dir, generation);
        }
        self.unsaved = 0;
        Ok(())
    }
}

fn remove_other_generations(dir: &Path, keep: u32) {
    let keep_name = format!("data-{keep}.bin");
    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("data-") && name.ends_with(".bin") && name != keep_name {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn encode_index(
    thumb_max: u32,
    scoring_version: u32,
    generation: u32,
    entries: &HashMap<String, Entry>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + entries.len() * 80);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&thumb_max.to_le_bytes());
    out.extend_from_slice(&scoring_version.to_le_bytes());
    out.extend_from_slice(&generation.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (name, e) in entries {
        let name = &name.as_bytes()[..name.len().min(u16::MAX as usize)];
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&e.stamp.size.to_le_bytes());
        out.extend_from_slice(&e.stamp.mtime_ns.to_le_bytes());
        out.extend_from_slice(&e.offset.to_le_bytes());
        out.extend_from_slice(&e.len.to_le_bytes());
        let s = e.score;
        for v in [
            s.overall,
            s.aesthetic.unwrap_or(f32::NAN),
            s.sharpness,
            s.exposure,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn parse_index(
    bytes: &[u8],
    thumb_max: u32,
    scoring_version: u32,
) -> Option<(u32, HashMap<String, Entry>)> {
    let mut r = bytes;
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic).ok()?;
    if &magic != MAGIC || read_u32(&mut r)? != thumb_max || read_u32(&mut r)? != scoring_version {
        return None;
    }
    let generation = read_u32(&mut r)?;
    let count = read_u32(&mut r)? as usize;
    let mut entries = HashMap::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        let mut len = [0u8; 2];
        r.read_exact(&mut len).ok()?;
        let mut name = vec![0u8; u16::from_le_bytes(len) as usize];
        r.read_exact(&mut name).ok()?;
        let entry = Entry {
            stamp: FileStamp {
                size: read_u64(&mut r)?,
                mtime_ns: read_u64(&mut r)?,
            },
            offset: read_u64(&mut r)?,
            len: read_u32(&mut r)?,
            score: Score {
                overall: read_f32(&mut r)?,
                aesthetic: Some(read_f32(&mut r)?).filter(|v| !v.is_nan()),
                sharpness: read_f32(&mut r)?,
                exposure: read_f32(&mut r)?,
            },
        };
        entries.insert(String::from_utf8(name).ok()?, entry);
    }
    r.is_empty().then_some((generation, entries))
}

fn read_u32(r: &mut &[u8]) -> Option<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}

fn read_f32(r: &mut &[u8]) -> Option<f32> {
    read_u32(r).map(f32::from_bits)
}

fn read_u64(r: &mut &[u8]) -> Option<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = file.seek_read(buf, offset)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ivt-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const S: Score = Score {
        overall: 7.5,
        aesthetic: Some(5.5),
        sharpness: 8.0,
        exposure: 9.0,
    };

    fn stamp(size: u64, mtime_ns: u64) -> FileStamp {
        FileStamp { size, mtime_ns }
    }

    #[test]
    fn insert_save_reopen_and_read_back() {
        let dir = temp_dir("roundtrip");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(10, 1), b"AAAA", S).unwrap();
        index.insert("b.jpg", stamp(20, 2), b"BBBBBB", S).unwrap();
        index.save().unwrap();
        drop(index);

        let index = ThumbIndex::open(&dir, 320, 1).unwrap();
        assert_eq!(index.len(), 2);
        let (off, len, _) = index.lookup("b.jpg", stamp(20, 2)).unwrap();
        assert_eq!(index.read(off, len).unwrap(), b"BBBBBB");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scores_roundtrip_including_missing_aesthetic() {
        let dir = temp_dir("scores");
        let no_model = Score {
            aesthetic: None,
            ..S
        };
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(1, 1), b"A", S).unwrap();
        index.insert("b.jpg", stamp(1, 1), b"B", no_model).unwrap();
        index.save().unwrap();
        drop(index);

        let index = ThumbIndex::open(&dir, 320, 1).unwrap();
        assert_eq!(index.lookup("a.jpg", stamp(1, 1)).unwrap().2, S);
        assert_eq!(index.score_of("b.jpg"), Some(no_model));
        assert_eq!(index.score_of("zzz.jpg"), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lookup_misses_when_file_changed() {
        let dir = temp_dir("stale");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(10, 1), b"A", S).unwrap();
        assert!(index.lookup("a.jpg", stamp(11, 1)).is_none());
        assert!(index.lookup("a.jpg", stamp(10, 2)).is_none());
        assert!(index.lookup("other.jpg", stamp(10, 1)).is_none());
        assert!(index.contains("a.jpg"));
        assert!(!index.contains("other.jpg"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unsaved_counts_inserts_until_save() {
        let dir = temp_dir("unsaved");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(1, 1), b"A", S).unwrap();
        index.insert("b.jpg", stamp(1, 1), b"B", S).unwrap();
        assert_eq!(index.unsaved(), 2);
        index.save().unwrap();
        assert_eq!(index.unsaved(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn different_thumb_size_or_corrupt_index_starts_empty() {
        let dir = temp_dir("corrupt");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(1, 1), b"A", S).unwrap();
        index.save().unwrap();
        drop(index);

        assert_eq!(ThumbIndex::open(&dir, 256, 1).unwrap().len(), 0);
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(1, 1), b"A", S).unwrap();
        index.save().unwrap();
        drop(index);
        assert_eq!(
            ThumbIndex::open(&dir, 320, 2).unwrap().len(),
            0,
            "otra versión de scoring"
        );

        fs::write(dir.join("index.bin"), b"garbage").unwrap();
        let index = ThumbIndex::open(&dir, 320, 1).unwrap();
        assert_eq!(index.len(), 0);
        assert_eq!(index.data_len(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entries_beyond_truncated_data_are_dropped() {
        let dir = temp_dir("truncated");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        index.insert("a.jpg", stamp(1, 1), b"AAAA", S).unwrap();
        index.insert("b.jpg", stamp(1, 1), b"BBBB", S).unwrap();
        index.save().unwrap();
        drop(index);

        OpenOptions::new()
            .write(true)
            .open(data_path(&dir, 0))
            .unwrap()
            .set_len(6)
            .unwrap();
        let index = ThumbIndex::open(&dir, 320, 1).unwrap();
        assert!(index.lookup("a.jpg", stamp(1, 1)).is_some());
        assert!(index.lookup("b.jpg", stamp(1, 1)).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_compacts_when_most_data_is_garbage() {
        let dir = temp_dir("compact");
        let mut index = ThumbIndex::open(&dir, 320, 1).unwrap();
        let big = vec![7u8; (COMPACT_MIN_BYTES / 4) as usize];
        for v in 0..5 {
            index.insert("a.jpg", stamp(1, v), &big, S).unwrap();
        }
        index.insert("b.jpg", stamp(1, 1), b"keep", S).unwrap();
        assert!(index.data_len() > COMPACT_MIN_BYTES);
        index.save().unwrap();

        assert_eq!(index.data_len(), big.len() as u64 + 4);
        let (off, len, _) = index.lookup("b.jpg", stamp(1, 1)).unwrap();
        assert_eq!(index.read(off, len).unwrap(), b"keep");
        let (off, len, _) = index.lookup("a.jpg", stamp(1, 4)).unwrap();
        assert_eq!(index.read(off, len).unwrap(), big);
        drop(index);
        let index = ThumbIndex::open(&dir, 320, 1).unwrap();
        assert_eq!(index.len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
