use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use crate::{SessionTape, SessionTapeError, TapeEntry, decode_persisted_line};

pub trait NamedTapeStorage {
    fn tape_names(&self) -> Vec<String>;
    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError>;
    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError>;
    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError>;
}

#[derive(Default)]
pub struct InMemoryTapeStorage {
    tapes: BTreeMap<String, Vec<TapeEntry>>,
}

impl InMemoryTapeStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl NamedTapeStorage for InMemoryTapeStorage {
    fn tape_names(&self) -> Vec<String> {
        self.tapes.keys().cloned().collect()
    }

    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError> {
        let Some(entries) = self.tapes.get(tape_name) else {
            return Ok(SessionTape::named(tape_name));
        };

        let mut tape = SessionTape::named(tape_name);
        for entry in entries.clone() {
            tape.load_persisted_entry(entry)?;
        }
        Ok(tape)
    }

    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError> {
        self.tapes.insert(tape.name.clone(), tape.entries.clone());
        Ok(())
    }

    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError> {
        self.tapes.entry(tape_name.to_string()).or_default().push(entry.clone());
        Ok(())
    }
}

pub struct JsonlTapeStorage {
    root_dir: PathBuf,
}

impl JsonlTapeStorage {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self { root_dir: root_dir.into() }
    }

    fn tape_path(&self, tape_name: &str) -> PathBuf {
        self.root_dir.join(format!("{tape_name}.jsonl"))
    }
}

impl NamedTapeStorage for JsonlTapeStorage {
    fn tape_names(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.root_dir) else {
            return Vec::new();
        };

        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                    return None;
                }
                path.file_stem().and_then(|value| value.to_str()).map(str::to_string)
            })
            .collect()
    }

    fn load_tape(&self, tape_name: &str) -> Result<SessionTape, SessionTapeError> {
        let path = self.tape_path(tape_name);
        if !path.exists() {
            return Ok(SessionTape::named(tape_name));
        }

        let file = fs::File::open(&path).map_err(SessionTapeError::from_io)?;
        let reader = BufReader::new(file);
        let mut tape = SessionTape::named(tape_name);
        for line in reader.lines() {
            let line = line.map_err(SessionTapeError::from_io)?;
            if line.trim().is_empty() {
                continue;
            }
            let entry = decode_persisted_line(&line)?;
            tape.load_persisted_entry(entry)?;
        }
        Ok(tape)
    }

    fn save_tape(&mut self, tape: &SessionTape) -> Result<(), SessionTapeError> {
        let path = self.tape_path(tape.tape_name());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SessionTapeError::from_io)?;
        }
        let contents = tape
            .entries
            .iter()
            .map(|entry| serde_json::to_string(entry).map_err(SessionTapeError::from_serde))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let contents = if contents.is_empty() { contents } else { format!("{contents}\n") };
        fs::write(path, contents).map_err(SessionTapeError::from_io)
    }

    fn append_entry_to(
        &mut self,
        tape_name: &str,
        entry: &TapeEntry,
    ) -> Result<(), SessionTapeError> {
        let path = self.tape_path(tape_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SessionTapeError::from_io)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(SessionTapeError::from_io)?;
        let line = serde_json::to_string(entry).map_err(SessionTapeError::from_serde)?;
        writeln!(file, "{line}").map_err(SessionTapeError::from_io)
    }
}
