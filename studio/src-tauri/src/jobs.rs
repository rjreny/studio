use crate::models::JobProgress;
use crate::storage::db::Database;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub type JobSlot = Arc<Mutex<Option<String>>>;

pub fn try_begin_job(slot: &Mutex<Option<String>>, name: &str) -> Result<(), String> {
    let mut guard = slot.lock().map_err(|e| e.to_string())?;
    if let Some(current) = guard.as_ref() {
        return Err(format!("Already running {current}"));
    }
    *guard = Some(name.to_string());
    Ok(())
}

pub fn end_job(slot: &Mutex<Option<String>>) {
    if let Ok(mut guard) = slot.lock() {
        *guard = None;
    }
}

pub fn spawn_job<F>(
    app: AppHandle,
    slot: JobSlot,
    db_path: PathBuf,
    name: &'static str,
    work: F,
) -> Result<(), String>
where
    F: FnOnce(&AppHandle, PathBuf) -> Result<(), String> + Send + 'static,
{
    try_begin_job(&slot, name).or_else(|err| {
        if err.contains("Already running") {
            std::thread::sleep(std::time::Duration::from_millis(80));
            try_begin_job(&slot, name)
        } else {
            Err(err)
        }
    })?;
    let _ = app.emit(
        "studio-job",
        JobProgress {
            job: name.into(),
            label: format!("Starting {name}…"),
            ..Default::default()
        },
    );
    let worker_slot = slot.clone();
    let thread = std::thread::Builder::new()
        .name(format!("studio-{name}"))
        .spawn(move || {
            let result = work(&app, db_path);
            if let Err(err) = result {
                crate::app_log::write(&app, &format!("{name} failed: {err}"));
                let _ = app.emit(
                    "studio-job",
                    JobProgress {
                        job: name.into(),
                        label: format!("{name} failed · {err}"),
                        errors: 1,
                        done: true,
                        ..Default::default()
                    },
                );
            }
            end_job(&worker_slot);
        })
        .map_err(|e| e.to_string());
    if let Err(err) = thread {
        end_job(&slot);
        return Err(err);
    }
    Ok(())
}

pub fn open_worker_db(path: &PathBuf) -> Result<Database, String> {
    Database::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_job_is_rejected_until_the_first_finishes() {
        let slot = Mutex::new(None);
        try_begin_job(&slot, "enrich").unwrap();
        let err = try_begin_job(&slot, "import").unwrap_err();
        assert!(err.contains("enrich"), "{err}");
        end_job(&slot);
        try_begin_job(&slot, "import").unwrap();
    }
}
