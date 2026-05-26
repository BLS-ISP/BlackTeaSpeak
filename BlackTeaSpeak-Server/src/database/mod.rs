use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use rusqlite::OptionalExtension;

pub mod schema;
pub mod servers;
pub mod channels;
pub mod groups;
pub mod clients;
pub mod bans;
pub mod tokens;

#[derive(Debug)]
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize_schema()?;
        Ok(db)
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize_schema()?;
        Ok(db)
    }
}
